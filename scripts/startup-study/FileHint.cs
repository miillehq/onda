using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.IO.MemoryMappedFiles;
using System.Runtime.InteropServices;
using System.Threading;

// Only loaded by the disposable measurement host. No code is loaded into Onda.
public static class OndaFileHintStudy {
    [StructLayout(LayoutKind.Sequential)]
    private struct Range { public IntPtr Start; public UIntPtr Length; }
    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool PrefetchVirtualMemory(IntPtr process, UIntPtr count, [In] Range[] ranges, uint flags);
    private static Thread worker;
    private static readonly List<string> results = new List<string>();
    public static void Start(string[] paths) {
        if (paths.Length == 0) return;
        worker = new Thread(() => {
            foreach (var path in paths) {
                var elapsed = Stopwatch.StartNew();
                try {
                    using (var file = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
                    using (var image = MemoryMappedFile.CreateFromFile(file, null, 0, MemoryMappedFileAccess.Read, HandleInheritability.None, true))
                    using (var view = image.CreateViewAccessor(0, Math.Min(file.Length, 16L*1024*1024), MemoryMappedFileAccess.Read)) {
                        var range = new Range { Start=view.SafeMemoryMappedViewHandle.DangerousGetHandle(), Length=new UIntPtr((ulong)view.Capacity) };
                        bool ok = PrefetchVirtualMemory(new IntPtr(-1), new UIntPtr(1), new[]{ range }, 0);
                        results.Add(Path.GetFileName(path) + " " + view.Capacity + " " + elapsed.ElapsedMilliseconds + " " + (ok?0:Marshal.GetLastWin32Error()));
                    }
                } catch (Exception e) { results.Add("error " + Path.GetFileName(path) + " " + e.GetType().Name); }
            }
        });
        worker.IsBackground = true;
        worker.Start();
    }
    public static string[] Complete() {
        if (worker != null && !worker.Join(8000)) return new [] {"still running"};
        return results.ToArray();
    }
}
