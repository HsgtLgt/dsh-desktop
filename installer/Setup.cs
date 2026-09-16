using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Reflection;
using System.Text;
using System.Threading;
using System.Windows.Forms;
using Microsoft.Win32;

/**
 * DeepSeek Harness 安装程序（图形化向导）。
 *
 * setup.exe 作为宿主，文件尾部依次拼接：
 *   payload.zip | uint32 载荷长度 | uint32 载荷文件数 | 15 字节尾标记
 *
 * 注意：csc 4.8 是 C# 5 编译器，不能用字符串插值、?.、表达式体成员等语法。
 */
internal static class Setup
{
    private static readonly byte[] Marker = Encoding.ASCII.GetBytes("DSHSFX1_PAYLOAD");

    [STAThread]
    private static void Main()
    {
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        try
        {
            Application.Run(new WizardForm());
        }
        catch (Exception error)
        {
            MessageBox.Show("安装程序启动失败：" + Environment.NewLine + error.Message,
                "DeepSeek Harness", MessageBoxButtons.OK, MessageBoxIcon.Error);
        }
    }

    internal static byte[] PayloadMarker()
    {
        return Marker;
    }
}

internal sealed class WizardForm : Form
{
    private readonly string selfPath;
    private readonly long payloadStart;
    private readonly long payloadLength;
    private readonly int payloadFiles;

    private readonly Panel[] pages;
    private int pageIndex;

    private TextBox pathBox;
    private Label pathNote;
    private CheckBox desktopShortcut;
    private CheckBox startMenuShortcut;
    private CheckBox launchAfter;
    private ProgressBar progressBar;
    private Label progressLabel;
    private Label doneTitle;

    private Button backButton;
    private Button nextButton;
    private Button cancelButton;

    private string installDir;
    private bool busy;
    private string failure;

    public WizardForm()
    {
        selfPath = new Uri(Assembly.GetExecutingAssembly().CodeBase).LocalPath;

        long start;
        long length;
        int files;
        ReadPayload(selfPath, out start, out length, out files);
        payloadStart = start;
        payloadLength = length;
        payloadFiles = files;

        installDir = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Programs", "DeepSeek Harness");

        Text = "DeepSeek Harness 安装程序";
        ClientSize = new Size(600, 442);
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false;
        MinimizeBox = false;
        StartPosition = FormStartPosition.CenterScreen;
        Font = new Font("Microsoft YaHei UI", 9F, FontStyle.Regular, GraphicsUnit.Point);
        BackColor = Color.White;

        pages = new Panel[5];
        BuildPages();
        BuildFooter();
        ShowPage(0);
    }

    // ── 载荷头解析 ──────────────────────────────────────────────
    private static void ReadPayload(string path, out long start, out long length, out int files)
    {
        start = -1;
        length = 0;
        files = 0;

        byte[] marker = Setup.PayloadMarker();
        int trailer = 8 + marker.Length;

        using (FileStream stream = File.OpenRead(path))
        {
            long total = stream.Length;
            if (total < trailer)
            {
                throw new Exception("安装包不完整。");
            }
            byte[] buffer = new byte[trailer];
            stream.Seek(total - trailer, SeekOrigin.Begin);
            int read = 0;
            while (read < trailer)
            {
                int step = stream.Read(buffer, read, trailer - read);
                if (step <= 0)
                {
                    break;
                }
                read += step;
            }
            for (int i = 0; i < marker.Length; i++)
            {
                if (buffer[8 + i] != marker[i])
                {
                    throw new Exception("安装包已损坏：未找到内置数据标记。");
                }
            }
            length = BitConverter.ToUInt32(buffer, 0);
            files = (int)BitConverter.ToUInt32(buffer, 4);
            start = total - trailer - length;
            if (start < 0)
            {
                throw new Exception("安装包已损坏：载荷长度异常。");
            }
        }
    }

    // ── 界面构建 ────────────────────────────────────────────────
    private void BuildPages()
    {
        for (int i = 0; i < pages.Length; i++)
        {
            Panel panel = new Panel();
            panel.Location = new Point(0, 78);
            panel.Size = new Size(600, 300);
            panel.BackColor = Color.White;
            panel.Visible = false;
            Controls.Add(panel);
            pages[i] = panel;
        }
        BuildHeader();
        BuildWelcome(pages[0]);
        BuildPathPage(pages[1]);
        BuildOptionsPage(pages[2]);
        BuildProgressPage(pages[3]);
        BuildDonePage(pages[4]);
    }

    private void BuildHeader()
    {
        Panel header = new Panel();
        header.Location = new Point(0, 0);
        header.Size = new Size(600, 78);
        header.BackColor = Color.FromArgb(247, 248, 250);
        Controls.Add(header);

        PictureBox icon = new PictureBox();
        icon.Location = new Point(24, 14);
        icon.Size = new Size(46, 46);
        icon.SizeMode = PictureBoxSizeMode.Zoom;
        try
        {
            Icon self = Icon.ExtractAssociatedIcon(selfPath);
            if (self != null)
            {
                icon.Image = self.ToBitmap();
            }
        }
        catch
        {
            // 取不到图标不影响安装
        }
        header.Controls.Add(icon);

        Label title = new Label();
        title.Text = "DeepSeek Harness";
        title.Font = new Font(Font.FontFamily, 14F, FontStyle.Bold);
        title.ForeColor = Color.FromArgb(23, 32, 51);
        title.Location = new Point(84, 16);
        title.AutoSize = true;
        header.Controls.Add(title);

        Label subtitle = new Label();
        subtitle.Text = "Windows 桌面版 · 对应官方 DSH 0.1.6-alpha.1";
        subtitle.ForeColor = Color.FromArgb(110, 118, 132);
        subtitle.Location = new Point(86, 46);
        subtitle.AutoSize = true;
        header.Controls.Add(subtitle);

        Panel line = new Panel();
        line.Location = new Point(0, 77);
        line.Size = new Size(600, 1);
        line.BackColor = Color.FromArgb(226, 229, 234);
        Controls.Add(line);
    }

    private void BuildWelcome(Panel page)
    {
        Label lead = new Label();
        lead.Text = "这个向导会把 DeepSeek Harness 安装到你的电脑上。";
        lead.Location = new Point(30, 22);
        lead.AutoSize = true;
        page.Controls.Add(lead);

        Label caption = new Label();
        caption.Text = "即将安装到：";
        caption.ForeColor = Color.FromArgb(110, 118, 132);
        caption.Location = new Point(30, 62);
        caption.AutoSize = true;
        page.Controls.Add(caption);

        Label target = new Label();
        target.Text = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Programs", "DeepSeek Harness");
        target.ForeColor = Color.FromArgb(23, 32, 51);
        target.Location = new Point(30, 84);
        target.AutoSize = true;
        page.Controls.Add(target);

        Label facts = new Label();
        facts.Text = "需要的磁盘空间：约 930 MB" + Environment.NewLine +
            "内置 Node.js 运行时，无需另行安装 Node 或 dsh" + Environment.NewLine +
            "安装到当前用户目录，不需要管理员权限";
        facts.ForeColor = Color.FromArgb(110, 118, 132);
        facts.Location = new Point(30, 124);
        facts.AutoSize = true;
        page.Controls.Add(facts);

        Label note = new Label();
        note.Text = "安装包未做数字签名，若 Windows 提示「未知发布者」，选择「仍要运行」即可。";
        note.ForeColor = Color.FromArgb(150, 110, 40);
        note.Location = new Point(30, 210);
        note.MaximumSize = new Size(540, 0);
        note.AutoSize = true;
        page.Controls.Add(note);
    }

    private void BuildPathPage(Panel page)
    {
        Label caption = new Label();
        caption.Text = "选择安装位置";
        caption.Font = new Font(Font.FontFamily, 11F, FontStyle.Bold);
        caption.Location = new Point(30, 18);
        caption.AutoSize = true;
        page.Controls.Add(caption);

        pathBox = new TextBox();
        pathBox.Location = new Point(30, 60);
        pathBox.Size = new Size(440, 26);
        pathBox.Text = installDir;
        pathBox.TextChanged += delegate { ValidatePath(); };
        page.Controls.Add(pathBox);

        Button browse = new Button();
        browse.Text = "浏览…";
        browse.Location = new Point(478, 59);
        browse.Size = new Size(80, 28);
        browse.Click += delegate { BrowseFolder(); };
        page.Controls.Add(browse);

        pathNote = new Label();
        pathNote.Location = new Point(30, 100);
        pathNote.MaximumSize = new Size(540, 0);
        pathNote.AutoSize = true;
        page.Controls.Add(pathNote);

        Label hint = new Label();
        hint.Text = "提示：路径越短越稳妥（深目录 + 长文件名容易触发个别工具的限制）。" +
            "安装程序内部已使用支持长路径的解压与复制方式，一般不受影响。";
        hint.ForeColor = Color.FromArgb(110, 118, 132);
        hint.Location = new Point(30, 150);
        hint.MaximumSize = new Size(540, 0);
        hint.AutoSize = true;
        page.Controls.Add(hint);

        ValidatePath();
    }

    private void BuildOptionsPage(Panel page)
    {
        Label caption = new Label();
        caption.Text = "安装选项";
        caption.Font = new Font(Font.FontFamily, 11F, FontStyle.Bold);
        caption.Location = new Point(30, 18);
        caption.AutoSize = true;
        page.Controls.Add(caption);

        desktopShortcut = new CheckBox();
        desktopShortcut.Text = "创建桌面快捷方式";
        desktopShortcut.Checked = true;
        desktopShortcut.Location = new Point(30, 64);
        desktopShortcut.AutoSize = true;
        page.Controls.Add(desktopShortcut);

        startMenuShortcut = new CheckBox();
        startMenuShortcut.Text = "创建开始菜单项";
        startMenuShortcut.Checked = true;
        startMenuShortcut.Location = new Point(30, 98);
        startMenuShortcut.AutoSize = true;
        page.Controls.Add(startMenuShortcut);

        Label hint = new Label();
        hint.Text = "安装完成后，可以从桌面、开始菜单或「应用和功能」中卸载。";
        hint.ForeColor = Color.FromArgb(110, 118, 132);
        hint.Location = new Point(30, 150);
        hint.MaximumSize = new Size(540, 0);
        hint.AutoSize = true;
        page.Controls.Add(hint);
    }

    private void BuildProgressPage(Panel page)
    {
        Label caption = new Label();
        caption.Text = "正在安装";
        caption.Font = new Font(Font.FontFamily, 11F, FontStyle.Bold);
        caption.Location = new Point(30, 18);
        caption.AutoSize = true;
        page.Controls.Add(caption);

        progressBar = new ProgressBar();
        progressBar.Location = new Point(30, 62);
        progressBar.Size = new Size(540, 18);
        page.Controls.Add(progressBar);

        progressLabel = new Label();
        progressLabel.Text = "准备中…";
        progressLabel.ForeColor = Color.FromArgb(60, 66, 78);
        progressLabel.Location = new Point(30, 92);
        progressLabel.MaximumSize = new Size(540, 0);
        progressLabel.AutoSize = true;
        page.Controls.Add(progressLabel);

        Label warn = new Label();
        warn.Text = "安装过程中请不要关闭这个窗口。";
        warn.ForeColor = Color.FromArgb(150, 110, 40);
        warn.Location = new Point(30, 150);
        warn.AutoSize = true;
        page.Controls.Add(warn);
    }

    private void BuildDonePage(Panel page)
    {
        doneTitle = new Label();
        doneTitle.Text = "安装完成";
        doneTitle.Font = new Font(Font.FontFamily, 11F, FontStyle.Bold);
        doneTitle.ForeColor = Color.FromArgb(22, 120, 70);
        doneTitle.Location = new Point(30, 18);
        doneTitle.AutoSize = true;
        page.Controls.Add(doneTitle);

        Label detail = new Label();
        detail.Text = "DeepSeek Harness 已经安装好了。";
        detail.Location = new Point(30, 60);
        detail.AutoSize = true;
        page.Controls.Add(detail);

        launchAfter = new CheckBox();
        launchAfter.Text = "立即启动 DeepSeek Harness";
        launchAfter.Checked = true;
        launchAfter.Location = new Point(30, 98);
        launchAfter.AutoSize = true;
        page.Controls.Add(launchAfter);

        Label hint = new Label();
        hint.Text = "首次启动会初始化桌面端专属配置，可能需要十几秒。";
        hint.ForeColor = Color.FromArgb(110, 118, 132);
        hint.Location = new Point(30, 140);
        hint.MaximumSize = new Size(540, 0);
        hint.AutoSize = true;
        page.Controls.Add(hint);
    }

    private void BuildFooter()
    {
        Panel footer = new Panel();
        footer.Location = new Point(0, 378);
        footer.Size = new Size(600, 64);
        footer.BackColor = Color.FromArgb(247, 248, 250);
        Controls.Add(footer);

        Panel line = new Panel();
        line.Location = new Point(0, 0);
        line.Size = new Size(600, 1);
        line.BackColor = Color.FromArgb(226, 229, 234);
        footer.Controls.Add(line);

        cancelButton = new Button();
        cancelButton.Text = "取消";
        cancelButton.Location = new Point(304, 18);
        cancelButton.Size = new Size(86, 30);
        cancelButton.Click += delegate { Close(); };
        footer.Controls.Add(cancelButton);

        backButton = new Button();
        backButton.Text = "上一步";
        backButton.Location = new Point(398, 18);
        backButton.Size = new Size(86, 30);
        backButton.Click += delegate { ShowPage(pageIndex - 1); };
        footer.Controls.Add(backButton);

        nextButton = new Button();
        nextButton.Text = "下一步";
        nextButton.Location = new Point(492, 18);
        nextButton.Size = new Size(86, 30);
        nextButton.Click += delegate { OnNext(); };
        footer.Controls.Add(nextButton);
    }

    // ── 页面切换与校验 ──────────────────────────────────────────
    private void ShowPage(int index)
    {
        if (index < 0 || index >= pages.Length)
        {
            return;
        }
        pages[pageIndex].Visible = false;
        pageIndex = index;
        pages[pageIndex].Visible = true;

        backButton.Visible = index > 0 && index < 3;
        backButton.Enabled = backButton.Visible && !busy;
        cancelButton.Text = index == 4 ? "关闭" : "取消";
        cancelButton.Enabled = !busy;

        if (index == 4)
        {
            nextButton.Visible = false;
            cancelButton.Location = new Point(492, 18);
        }
        else
        {
            nextButton.Visible = true;
            nextButton.Text = index == 2 ? "安装" : "下一步";
            nextButton.Enabled = index != 1 || IsPathValid();
        }
    }

    private bool IsPathValid()
    {
        if (pathBox == null)
        {
            return true;
        }
        string path = pathBox.Text.Trim();
        if (path.Length == 0 || path.IndexOfAny(Path.GetInvalidPathChars()) >= 0)
        {
            return false;
        }
        // 不按长度拦截：解压走 tar.exe、复制走 robocopy，两者都是原生长路径支持，
        // 不受 260 字符限制。过长的路径只在界面上给出提示。
        return true;
    }

    private void ValidatePath()
    {
        string path = pathBox.Text.Trim();
        if (path.Length == 0)
        {
            pathNote.Text = "请输入安装位置。";
            pathNote.ForeColor = Color.Firebrick;
        }
        else if (path.IndexOfAny(Path.GetInvalidPathChars()) >= 0)
        {
            pathNote.Text = "路径中含有非法字符。";
            pathNote.ForeColor = Color.Firebrick;
        }
        else if (path.Length > 100)
        {
            pathNote.Text = "路径偏长（当前 " + path.Length + " 字符），建议换一个短一些的目录。";
            pathNote.ForeColor = Color.FromArgb(150, 110, 40);
        }
        else
        {
            pathNote.Text = "需要约 930 MB 空间。";
            pathNote.ForeColor = Color.DimGray;
        }
        if (pageIndex == 1)
        {
            nextButton.Enabled = IsPathValid();
        }
    }

    private void BrowseFolder()
    {
        FolderBrowserDialog dialog = new FolderBrowserDialog();
        dialog.Description = "选择安装位置";
        dialog.ShowNewFolderButton = true;
        string current = pathBox.Text.Trim();
        if (current.Length > 0 && Directory.Exists(current))
        {
            dialog.SelectedPath = current;
        }
        if (dialog.ShowDialog(this) == DialogResult.OK)
        {
            pathBox.Text = Path.Combine(dialog.SelectedPath, "DeepSeek Harness");
        }
    }

    private void OnNext()
    {
        if (pageIndex == 2)
        {
            installDir = pathBox.Text.Trim();
            StartInstall();
            return;
        }
        if (pageIndex == 3)
        {
            return;
        }
        ShowPage(pageIndex + 1);
    }

    // ── 安装流程 ────────────────────────────────────────────────
    private void StartInstall()
    {
        busy = true;
        ShowPage(3);
        backButton.Enabled = false;
        cancelButton.Enabled = false;
        nextButton.Enabled = false;

        Thread worker = new Thread(RunInstall);
        worker.IsBackground = true;
        worker.Start();
    }

    private void RunInstall()
    {
        string stage = Path.Combine(Path.GetTempPath(), "dsh-setup-" + Guid.NewGuid().ToString("N"));
        try
        {
            Directory.CreateDirectory(stage);
            string zip = Path.Combine(stage, "payload.zip");

            Report(-1, "正在读取安装包…");
            ExtractPayload(zip);

            Report(-1, "正在解压应用文件（约 1-2 分钟）…");
            RunProcess("tar.exe", "-xf \"" + zip + "\" -C \"" + stage + "\"", stage);

            Report(0, "正在写入安装位置…");
            Directory.CreateDirectory(installDir);
            RunRobocopy(Path.Combine(stage, "win-unpacked"), installDir);

            string uninstaller = Path.Combine(stage, "uninstall.cmd");
            if (File.Exists(uninstaller))
            {
                File.Copy(uninstaller, Path.Combine(installDir, "uninstall.cmd"), true);
            }

            string exe = Path.Combine(installDir, "DeepSeek Harness.exe");
            if (!File.Exists(exe))
            {
                throw new Exception("主程序未就位：" + exe);
            }

            Report(-1, "正在创建快捷方式…");
            if (desktopShortcut.Checked)
            {
                CreateShortcut(Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory), exe);
            }
            if (startMenuShortcut.Checked)
            {
                CreateShortcut(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                    "Microsoft", "Windows", "Start Menu", "Programs"), exe);
            }
            WriteUninstallEntry();

            try
            {
                Directory.Delete(stage, true);
            }
            catch
            {
                // 临时目录清理失败不影响安装结果
            }
        }
        catch (Exception error)
        {
            failure = error.Message;
        }

        if (IsDisposed)
        {
            return;
        }
        try
        {
            BeginInvoke(new Action(FinishInstall));
        }
        catch
        {
            // 窗口已关闭
        }
    }

    private void FinishInstall()
    {
        busy = false;
        cancelButton.Enabled = true;
        if (failure != null)
        {
            doneTitle.Text = "安装未完成";
            doneTitle.ForeColor = Color.Firebrick;
            progressLabel.Text = failure;
            launchAfter.Checked = false;
            launchAfter.Enabled = false;
            ShowPage(4);
            return;
        }
        progressBar.Style = ProgressBarStyle.Continuous;
        progressBar.Value = 100;
        progressLabel.Text = "已完成。";
        ShowPage(4);
    }

    private void ExtractPayload(string destination)
    {
        using (FileStream source = File.OpenRead(selfPath))
        using (FileStream target = File.Create(destination))
        {
            source.Seek(payloadStart, SeekOrigin.Begin);
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
                int done = payloadLength > 0 ? (int)(100 - (100L * remaining / payloadLength)) : -1;
                Report(done, "正在读取安装包… " + done + "%");
            }
        }
    }

    private void RunProcess(string file, string arguments, string workingDirectory)
    {
        ProcessStartInfo info = new ProcessStartInfo(file, arguments);
        info.UseShellExecute = false;
        info.CreateNoWindow = true;
        info.RedirectStandardError = true;
        info.WorkingDirectory = workingDirectory;
        using (Process process = Process.Start(info))
        {
            string error = process.StandardError.ReadToEnd();
            process.WaitForExit();
            if (process.ExitCode != 0)
            {
                throw new Exception(file + " 执行失败（退出码 " + process.ExitCode + "）。"
                    + (error.Length > 0 ? Environment.NewLine + error.Substring(0, Math.Min(500, error.Length)) : ""));
            }
        }
    }

    private void RunRobocopy(string source, string destination)
    {
        string logPath = Path.Combine(Path.GetTempPath(), "dsh-copy-" + Guid.NewGuid().ToString("N") + ".log");
        string arguments = "\"" + source + "\" \"" + destination + "\""
            + " /E /R:1 /W:1 /NP /NDL /NJH /NJS /NC /NS /LOG:\"" + logPath + "\"";
        ProcessStartInfo info = new ProcessStartInfo("robocopy.exe", arguments);
        info.UseShellExecute = false;
        info.CreateNoWindow = true;

        using (Process process = Process.Start(info))
        {
            while (!process.HasExited)
            {
                Thread.Sleep(500);
                int copied = CountLines(logPath);
                int percent = payloadFiles > 0
                    ? (int)Math.Min(100L, 100L * copied / payloadFiles)
                    : -1;
                Report(percent, "正在写入安装位置… 已处理 " + copied + " / " + payloadFiles + " 个文件");
            }
            process.WaitForExit();
            if (process.ExitCode >= 8)
            {
                throw new Exception("复制文件失败，robocopy 退出码 " + process.ExitCode + "。");
            }
        }

        try
        {
            File.Delete(logPath);
        }
        catch
        {
            // 日志删除失败无妨
        }
    }

    private static int CountLines(string path)
    {
        try
        {
            using (FileStream stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite))
            using (StreamReader reader = new StreamReader(stream))
            {
                int count = 0;
                while (reader.ReadLine() != null)
                {
                    count++;
                }
                return count;
            }
        }
        catch
        {
            return 0;
        }
    }

    private void Report(int percent, string text)
    {
        if (IsDisposed)
        {
            return;
        }
        if (InvokeRequired)
        {
            try
            {
                BeginInvoke(new Action<int, string>(Report), percent, text);
            }
            catch
            {
                // 窗口正在关闭
            }
            return;
        }
        if (percent < 0)
        {
            progressBar.Style = ProgressBarStyle.Marquee;
        }
        else
        {
            progressBar.Style = ProgressBarStyle.Continuous;
            progressBar.Value = Math.Max(0, Math.Min(100, percent));
        }
        progressLabel.Text = text;
    }

    // ── 收尾动作 ────────────────────────────────────────────────
    private void CreateShortcut(string folder, string target)
    {
        try
        {
            Type shellType = Type.GetTypeFromProgID("WScript.Shell");
            if (shellType == null)
            {
                return;
            }
            object shell = Activator.CreateInstance(shellType);
            string linkPath = Path.Combine(folder, "DeepSeek Harness.lnk");
            object shortcut = shellType.InvokeMember("CreateShortcut", BindingFlags.InvokeMethod, null, shell,
                new object[] { linkPath });
            Type shortcutType = shortcut.GetType();
            shortcutType.InvokeMember("TargetPath", BindingFlags.SetProperty, null, shortcut, new object[] { target });
            shortcutType.InvokeMember("WorkingDirectory", BindingFlags.SetProperty, null, shortcut,
                new object[] { Path.GetDirectoryName(target) });
            shortcutType.InvokeMember("IconLocation", BindingFlags.SetProperty, null, shortcut, new object[] { target });
            shortcutType.InvokeMember("Description", BindingFlags.SetProperty, null, shortcut,
                new object[] { "DeepSeek Harness 桌面版" });
            shortcutType.InvokeMember("Save", BindingFlags.InvokeMethod, null, shortcut, null);
        }
        catch
        {
            // 快捷方式失败不阻断安装
        }
    }

    private void WriteUninstallEntry()
    {
        try
        {
            string key = @"Software\Microsoft\Windows\CurrentVersion\Uninstall\DeepSeekHarness";
            using (RegistryKey registry = Registry.CurrentUser.CreateSubKey(key))
            {
                if (registry == null)
                {
                    return;
                }
                registry.SetValue("DisplayName", "DeepSeek Harness");
                registry.SetValue("DisplayVersion", "0.1.6-alpha.1");
                registry.SetValue("Publisher", "DeepSeek");
                registry.SetValue("InstallLocation", installDir);
                registry.SetValue("UninstallString", "\"" + Path.Combine(installDir, "uninstall.cmd") + "\"");
                registry.SetValue("NoModify", 1, RegistryValueKind.DWord);
                registry.SetValue("NoRepair", 1, RegistryValueKind.DWord);
            }
        }
        catch
        {
            // 注册表写入失败不阻断安装
        }
    }

    protected override void OnFormClosing(FormClosingEventArgs e)
    {
        if (busy)
        {
            e.Cancel = true;
            return;
        }
        if (pageIndex == 4 && failure == null && launchAfter.Checked)
        {
            try
            {
                ProcessStartInfo info = new ProcessStartInfo(Path.Combine(installDir, "DeepSeek Harness.exe"));
                info.WorkingDirectory = installDir;
                info.UseShellExecute = true;
                Process.Start(info);
            }
            catch
            {
                // 启动失败时用户可手动打开
            }
        }
        base.OnFormClosing(e);
    }
}
