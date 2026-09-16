using System;
using System.Diagnostics;
using System.IO;
using System.IO.Compression;
using System.Reflection;
using System.Text;

internal static class Setup
{
    private static readonly byte[] Marker = Encoding.ASCII.GetBytes("DSHSFX1_PAYLOAD");

    private static int Main(string[] args)
    {
        Console.OutputEncoding = Encoding.UTF8;
        try
        {
            return Run();
        }
        catch (Exception error)
        {
            Console.WriteLine();
            Console.WriteLine("安装失败: " + error.Message);
            Console.WriteLine(error.StackTrace);
            Pause();
            return 1;
        }
    }

    private static int Run()
    {
        string self = new Uri(Assembly.GetExecutingAssembly().CodeBase).LocalPath;
        Console.WriteLine("DeepSeek Harness 安装程序");
        Console.WriteLine("--------------------------------");
        Console.WriteLine("正在读取安装包: " + self);

        long start;
        long payloadLength;
        using (FileStream probe = File.OpenRead(self))
        {
            start = FindPayloadStart(probe, out payloadLength);
        }
        if (start < 0)
        {
            Console.WriteLine("安装包已损坏：未找到内置数据。");
            Pause();
            return 1;
        }
        Console.WriteLine("数据偏移: " + start.ToString() + " 字节");

        string stage = Path.Combine(Path.GetTempPath(), "dsh-setup-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(stage);
        Console.WriteLine("正在解压到临时目录（约 930MB，需要几分钟，请勿关闭）...");
        Console.WriteLine("  " + stage);

        // 先把内置 zip 落成独立文件：.NET 的 ZipArchive 在内嵌流上会按绝对偏移解析
        // 中央目录，导致零条目。独立文件可正常读取。
        string payloadZip = Path.Combine(stage, "payload.zip");
        using (FileStream source = File.OpenRead(self))
        using (FileStream target = File.Create(payloadZip))
        {
            source.Seek(start, SeekOrigin.Begin);
            byte[] buffer = new byte[4 * 1024 * 1024];
            long remaining = payloadLength;
            while (remaining > 0)
            {
                int want = (int)Math.Min(buffer.Length, remaining);
                int read = source.Read(buffer, 0, want);
                if (read <= 0)
                {
                    break;
                }
                target.Write(buffer, 0, read);
                remaining -= read;
            }
        }

        int extracted = 0;
        using (System.Diagnostics.Process unpack = new System.Diagnostics.Process())
        {
            unpack.StartInfo.FileName = "tar.exe";
            unpack.StartInfo.Arguments = "-xf \"" + payloadZip + "\" -C \"" + stage + "\"";
            unpack.StartInfo.UseShellExecute = false;
            unpack.StartInfo.CreateNoWindow = true;
            unpack.StartInfo.RedirectStandardError = true;
            unpack.Start();
            string tarError = unpack.StandardError.ReadToEnd();
            unpack.WaitForExit();
            extracted = unpack.ExitCode;
            if (extracted != 0)
            {
                Console.WriteLine("tar 解压返回 " + extracted.ToString());
                if (tarError.Length > 0)
                {
                    Console.WriteLine(tarError.Substring(0, Math.Min(900, tarError.Length)));
                }
            }
        }
        // 不在这里递归统计：.NET 的路径规范化会拒绝超长路径，而 tar 已成功解压。
        // 大小由后续 install.ps1 报告。
        File.Delete(payloadZip);
        Console.WriteLine("解压完成。");

        string script = Path.Combine(stage, "install.ps1");
        if (!File.Exists(script))
        {
            Console.WriteLine("安装包缺少安装脚本: " + script);
            Pause();
            return 1;
        }

        Console.WriteLine("正在安装 ...");
        string log = Path.Combine(stage, "install.log");
        ProcessStartInfo info = new ProcessStartInfo();
        info.FileName = "cmd.exe";
        info.Arguments = "/c powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"" + script + "\" > \"" + log + "\" 2>&1";
        info.WorkingDirectory = stage;
        info.UseShellExecute = false;
        using (Process child = Process.Start(info))
        {
            child.WaitForExit();
            Console.WriteLine("安装脚本退出码: " + child.ExitCode.ToString());
        }
        if (File.Exists(log))
        {
            Console.WriteLine("---- 安装日志 ----");
            Console.WriteLine(File.ReadAllText(log));
            Console.WriteLine("------------------");
        }

        try
        {
            Directory.Delete(stage, true);
        }
        catch
        {
        }

        Console.WriteLine();
        Console.WriteLine("安装流程结束。");
        Pause();
        return 0;
    }

    private static long FindPayloadStart(FileStream probe, out long payloadLength)
    {
        payloadLength = 0;
        // 打包器把 8 字节长度 + 标记 追加在 zip 之后，标记是文件的尾签名。
        long length = probe.Length;
        int trailer = 8 + Marker.Length;
        if (length < trailer)
        {
            return -1;
        }
        byte[] buffer = new byte[trailer];
        probe.Seek(length - trailer, SeekOrigin.Begin);
        int read = 0;
        while (read < trailer)
        {
            int step = probe.Read(buffer, read, trailer - read);
            if (step <= 0)
            {
                break;
            }
            read += step;
        }
        if (read != trailer)
        {
            return -1;
        }
        for (int i = 0; i < Marker.Length; i++)
        {
            if (buffer[8 + i] != Marker[i])
            {
                return -1;
            }
        }
        uint declared = BitConverter.ToUInt32(buffer, 0);
        payloadLength = declared;
        long start = length - trailer - declared;
        return start >= 0 ? start : -1;
    }

    private static void Pause()
    {
        Console.WriteLine();
        Console.WriteLine("按回车键关闭 ...");
        try
        {
            Console.ReadLine();
        }
        catch
        {
        }
    }
}
