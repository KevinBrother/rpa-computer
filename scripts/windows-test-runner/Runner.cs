using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using System.Web.Script.Serialization;

namespace BoundedWindowsTests {
    // Restricted JSON grammar: the runner accepts exactly one flat object of strings,
    // string arrays and a positive integer. No deserializer's duplicate-key coercion.
    sealed class ConfigReader {
        readonly string text; int pos;
        public ConfigReader(string value) { text=value; }
        void Space() { while(pos<text.Length && " \r\n\t".IndexOf(text[pos])>=0) pos++; }
        bool Take(char c) { Space(); if(pos<text.Length && text[pos]==c) {pos++;return true;} return false; }
        void Need(char c) { if(!Take(c)) throw new ArgumentException("Expected " + c); }
        string String() {
            Space(); int start=pos; Need('"'); bool escaped=false;
            while(pos<text.Length) {
                char c=text[pos++];
                if(c<32) throw new ArgumentException("Control character in JSON string");
                if(!escaped && c=='"') return new JavaScriptSerializer().Deserialize<string>(text.Substring(start,pos-start));
                if(!escaped && c=='\\') escaped=true; else escaped=false;
            }
            throw new ArgumentException("Unterminated string");
        }
        public Dictionary<string,object> Read() {
            var result=new Dictionary<string,object>(StringComparer.Ordinal); Need('{');
            if(!Take('}')) { do {
                string key=String(); Need(':'); object value;
                if(result.ContainsKey(key)) throw new ArgumentException("Duplicate property: " + key);
                switch(key) {
                    case "executablePath": case "workingDirectory": case "evidenceDirectory": value=String(); break;
                    case "arguments":
                        var args=new List<string>(); Need('[');
                        if(!Take(']')) { do { args.Add(String()); if(args.Count>4096) throw new ArgumentException("Too many arguments"); } while(Take(',')); Need(']'); }
                        value=args.ToArray(); break;
                    case "timeoutSeconds":
                        Space(); int begin=pos; while(pos<text.Length && text[pos]>='0' && text[pos]<='9') pos++;
                        int n; string token=text.Substring(begin,pos-begin);
                        if(token.Length==0 || (token.Length>1 && token[0]=='0') || !int.TryParse(token,out n) || n<1 || n>3600) throw new ArgumentException("timeoutSeconds must be integer 1..3600");
                        value=n; break;
                    default: throw new ArgumentException("Unknown property: " + key);
                }
                result.Add(key,value);
            } while(Take(',')); Need('}'); }
            Space(); if(pos!=text.Length || result.Count!=5) throw new ArgumentException("Exactly five config properties required");
            return result;
        }
    }

    public static class Runner {
        const long LogCap=4*1024*1024;
        // Microsoft CRT argv convention: quote each arg, double backslashes before
        // quotes and before the closing quote. No shell or PowerShell expression.
        public static string EncodeArgument(string arg) {
            var b=new StringBuilder("\""); int slashes=0;
            foreach(char c in arg) {
                if(c=='\\') {slashes++;continue;}
                if(c=='"') {b.Append('\\',slashes*2+1);b.Append('"');}
                else {b.Append('\\',slashes);b.Append(c);} slashes=0;
            }
            b.Append('\\',slashes*2); b.Append('"'); return b.ToString();
        }
        sealed class Drain {
            public long Seen, Retained; public bool Truncated; public string Error; public Task Work;
            public Drain(Stream source,string path) {
                // Raw bytes: never decode/re-encode the child's output.
                Work=Task.Factory.StartNew(delegate {
                    try { using(var file=new FileStream(path,FileMode.CreateNew,FileAccess.Write,FileShare.Read)) {
                        var buffer=new byte[16384]; int read;
                        while((read=source.Read(buffer,0,buffer.Length))>0) {
                            Seen+=read; int keep=(int)Math.Min(read,LogCap-Retained);
                            if(keep>0) {file.Write(buffer,0,keep); Retained+=keep;}
                            if(keep<read) Truncated=true;
                        }
                    }} catch(Exception e) {Error=e.ToString();}
                },System.Threading.CancellationToken.None,TaskCreationOptions.LongRunning,TaskScheduler.Default);
            }
            public void Record(Dictionary<string,object> s,string prefix) {
                bool done=Work.IsCompleted; s[prefix+"_drain_complete"]=done;
                // Counters are read only after completion: no torn 64-bit reads on x86.
                s[prefix+"_bytes_seen"]=done ? (object)Seen : null;
                s[prefix+"_bytes_retained"]=done ? (object)Retained : null;
                s[prefix+"_truncated"]=done ? (object)Truncated : null;
                s[prefix+"_drain_error"]=done ? Error : "Drain not completed; file may still be open at runner exit";
            }
        }
        static int Remaining(Stopwatch clock,int limit) { return (int)Math.Max(0,limit-clock.ElapsedMilliseconds); }
        static string Absolute(string path) {
            if(string.IsNullOrWhiteSpace(path) || !Path.IsPathRooted(path) || path.StartsWith("\\\\",StringComparison.Ordinal) || path.Length<3 || path[1]!=':' || (path[2]!='\\' && path[2]!='/'))
                throw new ArgumentException("Local absolute drive path required");
            return Path.GetFullPath(path);
        }
        static string Hash(string path) { using(var sha=System.Security.Cryptography.SHA256.Create()) using(var stream=File.OpenRead(path)) return BitConverter.ToString(sha.ComputeHash(stream)).Replace("-", "").ToLowerInvariant(); }
        public static int Run(string configPath,string runnerSource,string moduleSource) {
            var s=new Dictionary<string,object>(); s["schema_version"]=1; s["native_exit_code"]=null;
            s["outcome"]="validation_failure"; s["timed_out"]=false; s["cleanup_state"]="not_started";
            s["descendant_containment"]="none";
            s["residual_child_limitation"]="Only the exact owned root Process handle can be killed. Descendants are not enumerated or contained; surviving descendants and inherited pipe handles are unknown. No claim of descendant cleanup.";
            s["runner_pid"]=Process.GetCurrentProcess().Id; s["runner_session_id"]=Process.GetCurrentProcess().SessionId;
            string evidence=null; Process p=null; Drain stdout=null,stderr=null; int status=70;
            try {
                if(Environment.OSVersion.Platform!=PlatformID.Win32NT) throw new ArgumentException("Windows required");
                s["runner_source_sha256"]=Hash(runnerSource); s["runner_module_sha256"]=Hash(moduleSource);
                string config=Absolute(configPath);
                var info=new FileInfo(config); if(!info.Exists || info.Length>1024*1024) throw new ArgumentException("Config missing or exceeds 1 MiB");
                string raw=File.ReadAllText(config,new UTF8Encoding(false,true));
                s["config_path"]=config; s["config_sha256"]=Hash(config);
                var c=new ConfigReader(raw).Read();
                string exe=Absolute((string)c["executablePath"]), cwd=Absolute((string)c["workingDirectory"]), dir=Absolute((string)c["evidenceDirectory"]);
                if(File.Exists(dir) || Directory.Exists(dir)) throw new ArgumentException("Evidence directory already exists; no overwrite");
                // Atomic reservation using CreateDirectoryW, not Directory.CreateDirectory's
                // succeeds-if-existing semantics (including competing runner invocations).
                if(!CreateDirectory(dir,IntPtr.Zero)) throw new IOException("Cannot reserve NEW evidence directory; Win32="+System.Runtime.InteropServices.Marshal.GetLastWin32Error());
                evidence=dir;
                File.WriteAllText(Path.Combine(dir,"config.json"),raw,new UTF8Encoding(false));
                if(!File.Exists(exe)) throw new ArgumentException("Executable missing");
                if(!Directory.Exists(cwd)) throw new ArgumentException("Working directory missing");
                s["executable_sha256"]=Hash(exe);
                var encoded=new List<string>(); foreach(string arg in (string[])c["arguments"]) { if(arg.IndexOf('\0')>=0) throw new ArgumentException("NUL is not representable in Windows argv"); encoded.Add(EncodeArgument(arg)); }
                string commandLine=string.Join(" ",encoded.ToArray());
                if(commandLine.Length+exe.Length+4>=32767) throw new ArgumentException("Windows command line too long");
                p=new Process(); p.StartInfo=new ProcessStartInfo(exe,commandLine) { WorkingDirectory=cwd,UseShellExecute=false,RedirectStandardOutput=true,RedirectStandardError=true,CreateNoWindow=true };
                s["outcome"]="launch_failure";
                var clock=Stopwatch.StartNew(); int limit=(int)c["timeoutSeconds"]*1000;
                if(!p.Start()) throw new IOException("Process.Start returned false");
                IntPtr owned=p.Handle; // Cache the OS handle before any wait/exit inspection.
                s["root_pid"]=p.Id; s["root_executable"]=exe;
                s["root_start_utc"]=p.StartTime.ToUniversalTime().ToString("o");
                try {s["root_session_id"]=p.SessionId;s["root_session_identity_source"]="Process.SessionId";}
                catch(InvalidOperationException) {
                    // Process.SessionId enumerates process metadata on Framework and can
                    // lose a very short-lived root. Creation uses the caller's token/session.
                    s["root_session_id"]=s["runner_session_id"];
                    s["root_session_identity_source"]="inherited_caller_session_no_alternate_token";
                }
                s["root_executable_identity_source"]="exact_absolute_ProcessStartInfo_FileName";
                File.WriteAllText(Path.Combine(dir,"identity.json"),new JavaScriptSerializer().Serialize(s),new UTF8Encoding(false));
                s["outcome"]="running"; s["cleanup_state"]="root_running";
                stdout=new Drain(p.StandardOutput.BaseStream,Path.Combine(dir,"stdout.log"));
                stderr=new Drain(p.StandardError.BaseStream,Path.Combine(dir,"stderr.log"));
                bool exited=p.WaitForExit(Remaining(clock,limit));
                if(exited) {
                    try { s["native_exit_code"]=p.ExitCode; s["cleanup_state"]="root_exited"; }
                    catch(Exception e) {s["native_exit_error"]=e.ToString();}
                }
                bool drained=Task.WaitAll(new Task[]{stdout.Work,stderr.Work},Remaining(clock,limit));
                if(!exited || !drained || clock.ElapsedMilliseconds>=limit) {
                    s["timed_out"]=true; s["outcome"]="timeout"; status=124;
                } else if(stdout.Error!=null || stderr.Error!=null) {s["outcome"]="drain_failure";status=74;}
                else if(s["native_exit_code"]==null) {s["outcome"]="unknown_native_exit";status=75;}
                else {int exit=(int)s["native_exit_code"];s["outcome"]=exit==0?"success":"native_failure";status=exit==0?0:1;}
            } catch(Exception e) {s["error"]=e.ToString();status=70;}
            finally {
                if(p!=null) {
                    try {
                        if(!p.HasExited) {p.Kill();s["cleanup_state"]="root_kill_requested";
                            // Fixed cleanup grace is separate from the configured execution/drain deadline.
                            if(p.WaitForExit(2000)) s["cleanup_state"]="root_exit_observed_after_kill";
                            else s["cleanup_state"]="root_exit_unconfirmed";
                        }
                        if(p.HasExited && s["native_exit_code"]==null) s["native_exit_code"]=p.ExitCode;
                    } catch(Exception e) {s["cleanup_error"]=e.ToString();s["cleanup_state"]="root_cleanup_unconfirmed";}
                    if(stdout!=null && stderr!=null) {
                        try {Task.WaitAll(new Task[]{stdout.Work,stderr.Work},250);} catch(Exception e) {s["cleanup_drain_error"]=e.ToString();}
                    }
                    if(stdout!=null) stdout.Record(s,"stdout"); if(stderr!=null) stderr.Record(s,"stderr");
                    // Do not dispose redirected streams with pending blocking reads: that can
                    // deadlock .NET Framework. Worker threads are background threads; host exits.
                    if((stdout==null || stdout.Work.IsCompleted) && (stderr==null || stderr.Work.IsCompleted)) p.Dispose();
                }
                s["harness_exit_code"]=status; s["finished_utc"]=DateTime.UtcNow.ToString("o");
                string json=new JavaScriptSerializer().Serialize(s);
                if(evidence!=null) {try {File.WriteAllText(Path.Combine(evidence,"summary.json"),json,new UTF8Encoding(false));} catch(Exception e) {Console.Error.WriteLine("Summary write failed: "+e);status=74;}}
                Console.Out.WriteLine(json);
            }
            return status;
        }
        [System.Runtime.InteropServices.DllImport("kernel32.dll",CharSet=System.Runtime.InteropServices.CharSet.Unicode,SetLastError=true,EntryPoint="CreateDirectoryW")]
        [return:System.Runtime.InteropServices.MarshalAs(System.Runtime.InteropServices.UnmanagedType.Bool)]
        static extern bool CreateDirectory(string path,IntPtr securityAttributes);
    }
}
