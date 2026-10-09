using System;
using System.Text;
using System.Threading;
class Child {
    static int Main(string[] args) {
        switch (args[0]) {
            case "exit": Console.WriteLine("retained-output"); Console.Error.WriteLine("retained-error"); return int.Parse(args[1]);
            case "argv": for (int i=1;i<args.Length;i++) Console.WriteLine(Convert.ToBase64String(Encoding.UTF8.GetBytes(args[i]))); return 0;
            case "stress": for(int i=0;i<8192;i++) { Console.WriteLine(new string('o',1024)); Console.Error.WriteLine(new string('e',1024)); } return 0;
            case "timeout": Console.WriteLine("before-timeout"); Thread.Sleep(30000); return 0;
            default: return 99;
        }
    }
}
