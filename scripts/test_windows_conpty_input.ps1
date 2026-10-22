#Requires -Version 7.0
# Run in a fresh PowerShell process; only synthetic input enters this private ConPTY.
# No desktop capture, user console injection or configuration reads.
# shortcut: expected records describe this Windows backend; revise only with new native mapping receipts.
$ErrorActionPreference = 'Stop'
$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) ('kcode-conpty-' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
$dll = Join-Path $fixtureRoot 'ConPtyFixture.dll'
$code = @'

using System;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Runtime.InteropServices;
using System.Diagnostics;
public static class ConPtyFixture {
 [StructLayout(LayoutKind.Sequential)] struct COORD { public short X,Y; public COORD(short x,short y){X=x;Y=y;} }
 [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct SI { public int cb;public string r,d,t;public int x,y,xs,ys,xc,yc,fill,flags;public short show,r2;public IntPtr rp,i,o,e; }
 [StructLayout(LayoutKind.Sequential)] struct SIX {public SI si;public IntPtr attrs;}
 [StructLayout(LayoutKind.Sequential)] struct PI {public IntPtr process,thread;public int pid,tid;}
 [StructLayout(LayoutKind.Explicit,Size=20)] struct RECORD {
 [FieldOffset(0)] public ushort type;[FieldOffset(4)] public int down;[FieldOffset(8)] public ushort repeat;
 [FieldOffset(10)] public ushort vk;[FieldOffset(12)] public ushort scan;[FieldOffset(14)] public char ch;[FieldOffset(16)] public uint state;
 }
 [DllImport("kernel32",SetLastError=true)] static extern bool CreatePipe(out IntPtr r,out IntPtr w,IntPtr sa,int size);
 [DllImport("kernel32")] static extern int CreatePseudoConsole(COORD size,IntPtr input,IntPtr output,uint flags,out IntPtr pc);
 [DllImport("kernel32")] static extern void ClosePseudoConsole(IntPtr pc);
 [DllImport("kernel32",SetLastError=true)] static extern bool InitializeProcThreadAttributeList(IntPtr list,int count,int flags,ref IntPtr size);
 [DllImport("kernel32",SetLastError=true)] static extern bool UpdateProcThreadAttribute(IntPtr list,uint flags,IntPtr attr,IntPtr value,IntPtr size,IntPtr old,IntPtr ret);
 [DllImport("kernel32")] static extern void DeleteProcThreadAttributeList(IntPtr list);
 [DllImport("kernel32",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool CreateProcessW(string app,StringBuilder command,IntPtr pa,IntPtr ta,bool inherit,uint flags,IntPtr env,string cwd,ref SIX si,out PI pi);
 [DllImport("kernel32",SetLastError=true)] static extern bool ReadFile(IntPtr h,byte[] buffer,int size,out int count,IntPtr ov);
 [DllImport("kernel32",SetLastError=true)] static extern bool WriteFile(IntPtr h,byte[] buffer,int size,out int count,IntPtr ov);
 [DllImport("kernel32")] static extern bool CloseHandle(IntPtr h);
 [DllImport("kernel32")] static extern uint WaitForSingleObject(IntPtr h,uint ms);
 [DllImport("kernel32")] static extern bool TerminateProcess(IntPtr h,uint code);
 [DllImport("kernel32")] static extern IntPtr GetStdHandle(int which);
 [DllImport("kernel32",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr CreateFileW(string name,uint access,uint share,IntPtr security,uint creation,uint flags,IntPtr template);
 static IntPtr childOutput;
 static void Log(string value){byte[] data=Encoding.UTF8.GetBytes(value+"\r\n");int count;Check(WriteFile(childOutput,data,data.Length,out count,IntPtr.Zero)&&count==data.Length,"Child output");}
 [DllImport("kernel32",SetLastError=true)] static extern bool GetConsoleMode(IntPtr h,out uint mode);
 [DllImport("kernel32",SetLastError=true)] static extern bool SetConsoleMode(IntPtr h,uint mode);
 [DllImport("kernel32",SetLastError=true)] static extern bool GetNumberOfConsoleInputEvents(IntPtr h,out uint count);
 [DllImport("kernel32",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool ReadConsoleInputW(IntPtr h,[Out] RECORD[] records,uint length,out uint count);
 static void Check(bool ok,string name){if(!ok)throw new Exception(name+": "+Marshal.GetLastWin32Error());}
 public static void Child(){
  IntPtr h=CreateFileW("CONIN$",0xC0000000,3,IntPtr.Zero,3,0,IntPtr.Zero);childOutput=CreateFileW("CONOUT$",0xC0000000,3,IntPtr.Zero,3,0,IntPtr.Zero);uint original;Check(GetConsoleMode(h,out original),"GetConsoleMode");
  uint mode=(original & ~(uint)(1|2|4|0x200))|0x80;Check(SetConsoleMode(h,mode),"SetConsoleMode");
  uint effective;Check(GetConsoleMode(h,out effective),"GetConsoleMode effective");
  Log("\x1b[?2004h");
  Log("READY original="+original+" effective="+effective);
  var watch=Stopwatch.StartNew();int total=0;
  while(watch.ElapsedMilliseconds<1400){
   uint count;Check(GetNumberOfConsoleInputEvents(h,out count),"GetNumberOfConsoleInputEvents");
   if(count==0){Thread.Sleep(5);continue;}
   RECORD[] records=new RECORD[Math.Min((int)count,128)];uint read;Check(ReadConsoleInputW(h,records,(uint)records.Length,out read),"ReadConsoleInputW");
   for(int i=0;i<read;i++){var r=records[i];if(r.type!=1)continue;Log("KEY "+r.down+" "+r.repeat+" "+r.vk+" "+((int)r.ch)+" "+r.state);if(++total>4096)throw new Exception("record cap");}
  }
  Log("DONE "+total);
 }
 public static string Run(string pwsh,string dll,string[] chunks,int delay){
  IntPtr ir=IntPtr.Zero,iw=IntPtr.Zero,or=IntPtr.Zero,ow=IntPtr.Zero,pc=IntPtr.Zero,attrs=IntPtr.Zero;
  PI pi=new PI();Task pump=null;var output=new StringBuilder();Exception pumpError=null;
  try{
   Check(CreatePipe(out ir,out iw,IntPtr.Zero,0),"CreatePipe input");Check(CreatePipe(out or,out ow,IntPtr.Zero,0),"CreatePipe output");
   int hr=CreatePseudoConsole(new COORD(120,30),ir,ow,0,out pc);if(hr!=0)throw new Exception("CreatePseudoConsole HRESULT="+hr);
   CloseHandle(ir);ir=IntPtr.Zero;CloseHandle(ow);ow=IntPtr.Zero;
   IntPtr bytes=IntPtr.Zero;InitializeProcThreadAttributeList(IntPtr.Zero,1,0,ref bytes);attrs=Marshal.AllocHGlobal(bytes);
   Check(InitializeProcThreadAttributeList(attrs,1,0,ref bytes),"Initialize attributes");
   Check(UpdateProcThreadAttribute(attrs,0,new IntPtr(0x20016),pc,new IntPtr(IntPtr.Size),IntPtr.Zero,IntPtr.Zero),"Update HPCON");
   SIX sx=new SIX();sx.si.cb=Marshal.SizeOf(typeof(SIX));sx.attrs=attrs;
   string child="Add-Type -Path '"+dll.Replace("'","''")+"'; [ConPtyFixture]::Child()";
   string encoded=Convert.ToBase64String(Encoding.Unicode.GetBytes(child));
   Check(CreateProcessW(pwsh,new StringBuilder("\""+pwsh+"\" -NoLogo -NoProfile -EncodedCommand "+encoded),IntPtr.Zero,IntPtr.Zero,false,0x80000,IntPtr.Zero,null,ref sx,out pi),"Create child");
   IntPtr outputHandle=or;
   pump=Task.Run(()=>{try{byte[] buf=new byte[2048];int n;while(ReadFile(outputHandle,buf,buf.Length,out n,IntPtr.Zero)&&n>0){lock(output){output.Append(Encoding.UTF8.GetString(buf,0,n));if(output.Length>65536){output.Length=65536;pumpError=new Exception("output cap");}}}}catch(Exception e){pumpError=e;}});
   var ready=Stopwatch.StartNew();bool isReady=false;
   while(ready.ElapsedMilliseconds<8000){lock(output){isReady=output.ToString().Contains("READY ");}if(isReady)break;Thread.Sleep(10);}
   if(!isReady)throw new Exception("Child readiness timeout: "+output);
   foreach(string chunk in chunks){byte[] b=Encoding.UTF8.GetBytes(chunk);int written;Check(WriteFile(iw,b,b.Length,out written,IntPtr.Zero)&&written==b.Length,"Write private input");if(delay>0)Thread.Sleep(delay);}
   if(WaitForSingleObject(pi.process,4000)!=0)throw new Exception("Owned child timeout");
   ClosePseudoConsole(pc);pc=IntPtr.Zero;if(!pump.Wait(2000))throw new Exception("Output drain timeout");if(pumpError!=null)throw pumpError;
   string result;lock(output){result=output.ToString();}if(!result.Contains("DONE "))throw new Exception("Child incomplete: "+result);return result;
  }finally{
   if(pi.process!=IntPtr.Zero){if(WaitForSingleObject(pi.process,0)!=0)TerminateProcess(pi.process,124);CloseHandle(pi.process);}if(pi.thread!=IntPtr.Zero)CloseHandle(pi.thread);
   if(pc!=IntPtr.Zero)ClosePseudoConsole(pc);if(iw!=IntPtr.Zero)CloseHandle(iw);if(ir!=IntPtr.Zero)CloseHandle(ir);if(ow!=IntPtr.Zero)CloseHandle(ow);if(or!=IntPtr.Zero)CloseHandle(or);
   if(attrs!=IntPtr.Zero){DeleteProcThreadAttributeList(attrs);Marshal.FreeHGlobal(attrs);}
  }
 }
}

'@
Add-Type -TypeDefinition $code -OutputAssembly $dll
Add-Type -Path $dll
$pwsh = (Get-Process -Id $PID).Path
$esc = [string][char]27
$lf = [string][char]10
$cr = [string][char]13
$del = [string][char]127
$cases = @(
 @{Name='char'; Chunks=@('a'); Delay=0; Expected=@(97)},
 @{Name='enter'; Chunks=@($cr); Delay=0; Expected=@(13)},
 @{Name='escape'; Chunks=@($esc); Delay=0; Expected=@(27)},
 @{Name='backspace'; Chunks=@($del); Delay=0; Expected=@(8)},
 @{Name='ordinary'; Chunks=@('a',$cr,$esc,$del); Delay=100; Expected=@(97,13,27,8)},
 @{Name='short-framed-lf'; Chunks=@($esc+'[200~a'+$lf+'b'+$lf+$esc+'[201~'); Delay=0; Expected=@(97,10,98,10)},
 @{Name='short-framed-cr'; Chunks=@($esc+'[200~a'+$cr+'b'+$cr+$esc+'[201~'); Delay=0; Expected=@(97,13,98,13)},
 @{Name='leading-newline'; Chunks=@($esc+'[200~'+$lf+'a'+$lf+$esc+'[201~'); Delay=0; Expected=@(10,97,10)},
 @{Name='start-split'; Chunks=@($esc,('[200~a'+$lf+'b'+$lf+$esc+'[201~')); Delay=30; Expected=@(27,91,50,48,48,126,97,10,98,10)},
 @{Name='end-split'; Chunks=@(($esc+'[200~a'+$lf+'b'+$lf+$esc),'[201~'); Delay=30; Expected=@(97,10,98,10,27,91,50,48,49,126)},
 @{Name='split-frame'; Chunks=@($esc,'[200~a',($lf+'b'+$lf),$esc,'[201~'); Delay=30; Expected=@(27,91,50,48,48,126,97,10,98,10,27,91,50,48,49,126)},
 @{Name='delayed-frame'; Chunks=@(($esc+'[200~a'),($lf+'b'+$lf),($esc+'[201~')); Delay=250; Expected=@(97,10,98,10)}
)
foreach ($case in $cases) {
 $result = [ConPtyFixture]::Run($pwsh,$dll,[string[]]$case.Chunks,$case.Delay)
 [IO.File]::WriteAllText((Join-Path $fixtureRoot ($case.Name+'.log')),$result)
 $records = [regex]::Matches($result,'KEY (\d+) (\d+) (\d+) (\d+) (\d+)')
 if ($records.Count -eq 0) { throw "No key records: $($case.Name)" }
 $down = @($records | Where-Object {$_.Groups[1].Value -eq '1' -and $_.Groups[4].Value -ne '0'} | ForEach-Object {[int]$_.Groups[4].Value})
 if (($down -join ',') -ne ($case.Expected -join ',')) {
     throw ("Unexpected native mapping "+$case.Name+": "+($down -join ','))
 }
 if ($result -notmatch 'READY original=(\d+) effective=(\d+)') { throw 'Missing native mode receipt' }
 $expectedMode = ([int]$Matches[1] -band (-bnot 519)) -bor 128
 if ([int]$Matches[2] -ne $expectedMode) { throw 'Native console mode mismatch' }
 $keyDown = @($records | Where-Object {$_.Groups[1].Value -eq '1' -and $_.Groups[4].Value -ne '0'})
 foreach ($record in $keyDown) {
     $u = [int]$record.Groups[4].Value
     $expectedState = if ($u -eq 10) {8} elseif ($u -eq 126) {16} else {0}
     $expectedVk = switch ($u) {97 {65} 98 {66} 10 {13} 13 {13} 27 {27} 8 {8} 91 {219} 126 {192} default {$u}}
     if ([int]$record.Groups[2].Value -ne 1 -or [int]$record.Groups[3].Value -ne $expectedVk -or [int]$record.Groups[5].Value -ne $expectedState) {
         throw ('Unexpected repeat/VKey/control state: '+$record.Value)
     }
 }
 Write-Output ('NATIVE_DOWN '+(($keyDown | ForEach-Object {$_.Groups[3].Value+':'+$_.Groups[4].Value+':'+$_.Groups[5].Value}) -join ','))
 Write-Output ("PASS "+$case.Name+" RECORDS="+$records.Count+" DOWN_UTF16="+($down -join ','))
}
Write-Output "LIMIT: maps this Windows ConPTY backend only; does not exercise crossterm, app submission, focus/resume, HERDR or Mac."
Write-Output "RECEIPTS $fixtureRoot"
