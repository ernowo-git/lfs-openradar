# LFS startup setup

OpenRadar can configure your selected LFS installation to enable InSim whenever
LFS starts. LFS runs `data/script/autoexec.lfs` at startup; OpenRadar adds or
updates the `/insim` command there. See the
[LFS script guide](https://en.lfsmanual.net/wiki/Script_Guide).

## Enable InSim at startup

1. Open **LFS startup setup** in the OpenRadar control panel.
2. Click **Browse…**, navigate to your LFS installation, and click **Choose this
   folder**. You can also paste the folder path directly into **LFS folder**.
   Select the folder containing `LFS.exe`, with its existing `data/script`
   directory, rather than OpenRadar's folder.
3. Check the displayed startup command. It uses the port from OpenRadar's
   `insim_address`, normally `/insim 29999`.
4. Click **Enable InSim at startup**. The result shows the script path and, when
   an existing script was edited, its backup path.
5. Restart LFS to execute the updated startup script. If you need the listener
   immediately without restarting, type the displayed `/insim` command in LFS.
6. Click **Save settings** to remember the selected installation folder.

Choosing a folder, launching OpenRadar, or saving settings does not edit LFS.
The setup button is the explicit action that changes its startup script. It does
not modify LFS's `cfg.txt`, OutSim settings, or LFSLapper configuration.

The same folder browser works on Windows and Linux. For Wine/Proton, select the
actual LFS installation inside the prefix, containing `LFS.exe` and `data/script`.
This setup feature has been built and tested on Windows; Linux manual validation
remains pending.

## Existing commands and port conflicts

If the script already enables the configured port, setup reports that it is
already configured. It does not rewrite the script or create another backup.

If a different port is present, OpenRadar shows the conflict before editing:

- **Replace with OpenRadar port …** changes the script's existing standalone
  InSim commands to OpenRadar's configured port, after creating a backup.
- **Use existing port …** adopts the script's port in OpenRadar without changing
  the script. In live mode, OpenRadar reconnects using that port. Click **Save
  settings** to persist the choice. This option is available when the script
  has one distinct nonzero InSim port.
- **Cancel** leaves the script and OpenRadar's port unchanged.

Multiple conflicting commands are listed together. Replacement preserves each
command's position and surrounding content while making their ports agree. A
comment mentioning `/insim` is not treated as an active command.

## Backups and preservation

Before editing an existing file, OpenRadar creates a uniquely named backup next
to it, such as `autoexec.lfs.openradar-<timestamp>-<number>.bak`. Earlier backups
are never overwritten. If `autoexec.lfs` is missing, setup creates it; there is
no previous file to back up.

Unrelated commands, comments, whitespace, line endings, and existing ANSI or
UTF-8 bytes are preserved. The updated script is written to a temporary file in
the same directory, then replaces the original. If the script changes between
inspection and application, setup refuses to overwrite that edit; inspect again.

To undo an edit, close LFS, save any later changes you want to keep, and restore
the reported `.bak` file's contents to `data/script/autoexec.lfs`. Restart LFS.
Removing the `/insim` command also stops enabling the listener on future launches.

## When setup cannot proceed

- Select a complete LFS installation, not its parent folder or a launcher folder.
- The script directory must be writable. Read-only files and permission errors
  produce a setup error.
- Place each `/insim` command on its own line. Combined command lines or
  unrecognized port values require manual editing.
- UTF-16 scripts are not updated; use the usual ANSI or UTF-8 LFS script format.
- Setup rejects script links and script directories outside the selected
  installation rather than writing through them.

For an alternative that leaves the startup script unchanged, append
`/insim=29999` to your LFS shortcut's executable command. This applies when
launching through that shortcut. See the
[LFS startup options](https://en.lfsmanual.net/wiki/Commands).
