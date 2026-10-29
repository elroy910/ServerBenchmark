# Windows: transfers and comparisons with WSL

Run uploads/downloads in **Windows PowerShell**, whose prompt looks like `PS C:\Users\YourName>`. A prompt such as `leroy@server:~$` is Linux, including an SSH or WSL session.

`scp` saves the destination on the computer where it runs. `$HOME` is the home directory for that shell. In PowerShell it normally means `C:\Users\YourName`; in Linux it means `/home/your-user`.

## Download server results into Windows Downloads

Run on Windows, replacing the login, address and filename:

```powershell
New-Item -ItemType Directory -Force -Path "$HOME\Downloads" | Out-Null
scp USER@SERVER_IP:~/serverbench/results/server-a.json "$HOME\Downloads\server-a.json"
Get-Item "$HOME\Downloads\server-a.json"
```

A destination such as `.\server-a.json` instead saves to PowerShell's current folder. `Get-Location` shows that folder.

If you see an error such as `open local "/home/user\\Downloads\\server-a.json": Permission denied`, the Windows command ran in Linux. Open a new Windows PowerShell tab and run it there. If you entered SSH from PowerShell, `exit` returns to the previous shell; verify the prompt.

## Compare locally with WSL

Start an existing Ubuntu WSL distribution from PowerShell:

```powershell
wsl -d Ubuntu
```

Inside Ubuntu, download the Linux executable using the README's `wget` instructions. Then compare Windows files through `/mnt/c`:

```bash
./serverbench compare \
  "/mnt/c/Users/YOUR_WINDOWS_USERNAME/Downloads/server-a.json" \
  "/mnt/c/Users/YOUR_WINDOWS_USERNAME/Downloads/server-b.json"
```

Replace the Windows username in both paths. `C:\` maps to `/mnt/c/`, `D:\` maps to `/mnt/d/`. Quote paths containing spaces. Compare only reads saved results; the Linux executable does not run directly as a native Windows program.

## Manual upload instead of wget

From local PowerShell, copy a matching built executable to a Linux server:

```powershell
ssh USER@SERVER_IP "mkdir -p ~/serverbench"
scp "PATH\TO\serverbench-linux-x86_64" USER@SERVER_IP:~/serverbench/serverbench
ssh USER@SERVER_IP
```

Inside that SSH session:

```bash
cd ~/serverbench
chmod +x serverbench
./serverbench validate
./serverbench benchmark --full --label "server-a" --output results/server-a.json
```

For an ARM server, copy `serverbench-linux-arm64` instead. Only the executable is required; its datasets/reference are embedded.
