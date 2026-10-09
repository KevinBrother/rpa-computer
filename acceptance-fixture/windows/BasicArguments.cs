// Pure, strict argument parsing. All console modes exit before WinForms initialization.
using System;
using System.Collections.Generic;
using System.Globalization;
namespace AcceptanceFixture {
    sealed class BasicArguments {
        public string Suite="legacy",EvidencePath,ExportPath,PlatformExportPath,FocusExportPath,GeometryExportPath;
        public ulong? Seed;
        public bool SelfTest;
        public static BasicArguments Parse(string[] args) {
            var result=new BasicArguments();var seen=new HashSet<string>(StringComparer.Ordinal);
            for(int i=0;i<args.Length;i++) {
                string item=args[i];int equal=item.IndexOf('=');
                string name=equal<0?item:item.Substring(0,equal);
                if(!seen.Add(name)) throw new ArgumentException("duplicate argument: "+name);
                if(name=="--self-test") {
                    if(equal>=0) throw new ArgumentException("--self-test takes no value");
                    result.SelfTest=true;continue;
                }
                if(name!="--suite"&&name!="--seed"&&name!="--evidence-file"&&name!="--export-cases"&&name!="--export-platform-cases"&&name!="--export-focus-cases"&&name!="--export-geometry-cases")
                    throw new ArgumentException("unknown argument: "+name);
                string value;
                if(equal>=0)value=item.Substring(equal+1);
                else {if(i+1>=args.Length||args[i+1].StartsWith("--",StringComparison.Ordinal)) throw new ArgumentException(name+" requires a value");value=args[++i];}
                if(string.IsNullOrWhiteSpace(value))throw new ArgumentException(name+" requires a nonempty value");
                if(name=="--suite")result.Suite=value;
                else if(name=="--seed") {
                    ulong n;if(!ulong.TryParse(value,NumberStyles.None,CultureInfo.InvariantCulture,out n))throw new ArgumentException("--seed must be an unsigned 64-bit integer");result.Seed=n;
                } else if(name=="--evidence-file")result.EvidencePath=value;
                else if(name=="--export-cases")result.ExportPath=value;
                else if(name=="--export-platform-cases")result.PlatformExportPath=value;
                else if(name=="--export-focus-cases")result.FocusExportPath=value;
                else result.GeometryExportPath=value;
            }
            SuiteKind old;
            if(result.Suite!="geometry"&&result.Suite!="focus"&&!BasicCatalog.IsBasic(result.Suite)&&!GestureCatalog.IsGesture(result.Suite)&&!SuiteNames.TryParse(result.Suite,out old))
                throw new ArgumentException("unknown suite: "+result.Suite);
            if((result.SelfTest?1:0)+(result.ExportPath!=null?1:0)+(result.PlatformExportPath!=null?1:0)+(result.FocusExportPath!=null?1:0)+(result.GeometryExportPath!=null?1:0)>1)
                throw new ArgumentException("choose exactly one console mode: self-test, export-cases, export-platform-cases, export-focus-cases, export-geometry-cases");
            return result;
        }
    }
}
