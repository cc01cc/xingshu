# Xingshu (xs) quick start

`xs` indexes the Git repositories on your machine so you can search them all from one place.
It needs **Git installed and on your `PATH`** — `xs` calls the system `git` binary for
scanning, indexing and `grep`, and prints an install hint when Git cannot be found.

- Release: `{{VERSION}}`
- Platform: `windows-msvc` (Windows 10+) or `unknown-linux-gnu` (glibc 2.35+), per the archive name
- Signature: none. The build is unsigned, so verify integrity with the release `SHA256SUMS` and nothing more

## 1. Check the binary

```console
$ xs --version
xs {{VERSION}}
```

`xs --version` must print exactly the line above. If you have not put the binary on `PATH`
yet, run it in place: `./xs` on Linux, `.\xs.exe` on Windows. The rest of this file assumes
`xs` resolves on `PATH` (section 2).

## 2. Verify the download and put `xs` on `PATH`

Verify the archive against the `SHA256SUMS` file published with the release (it lists both
archives, so pick the line for the file you downloaded):

```console
$ grep 'x86_64-unknown-linux-gnu.tar.gz$' SHA256SUMS | sha256sum -c -        # Linux
$ (Select-String -Path SHA256SUMS -Pattern 'windows-msvc\.zip$').Line        # Windows: compare by eye
$ (Get-FileHash .\xingshu-v{{VERSION}}-x86_64-pc-windows-msvc.zip -Algorithm SHA256).Hash
```

Then install:

- **Windows**: create a directory such as `%LOCALAPPDATA%\Programs\xingshu`, move `xs.exe`
  there, and add that directory to your user `PATH`.
- **Linux**: `mkdir -p ~/.local/bin && mv xs ~/.local/bin/ && chmod +x ~/.local/bin/xs`
  (`~/.local/bin` is usually already on `PATH`; add it if a new shell cannot find `xs`).

## 3. Register a root and scan it

A *root* is a directory tree that contains Git repositories.

```console
$ xs roots add ./repos
registered root 1
$ xs roots list
```

`xs roots list` prints JSON. Copy the `path` field of the root you just added **verbatim** —
`xs scan --root` compares it literally, with no path normalization (same separator, same
case, no added or removed trailing separator):

```console
$ xs scan --root ./repos
```

`--root` here is a **filesystem path**, not a root id. Passing the id fails:

```console
$ xs scan --root 1
Error: root is not registered: 1
```

## 4. See what was found

```console
$ xs list
```

Every repository has an `org` and a `name`; `org/name` (for example `local/demo`) is how you
address it in `xs read` below.

## 5. Index and search

```console
$ xs index update --root 1
$ xs files '**/*.md' --root 1
$ xs search "needle" --root 1
$ xs grep "needle" --all-roots
$ xs read local/demo README.md
```

- `xs files` and `xs search` read the local full-text index; `xs index update --root <ID>`
  refreshes that index for one root (`--all-roots` covers every root).
- `xs grep` reads the Git working tree (or `--ref <GIT_REF>`) and therefore needs Git.
- On `index`/`files`/`search`/`grep` the `--root` value is the **root id** (the integer from
  `xs roots list`), unlike `xs scan --root <PATH>`.
- `--json` on `files`/`search`/`grep`/`read` prints machine-readable output.

## 6. Where your data lives

```console
$ xs db path
```

- Windows: `%LOCALAPPDATA%\Xingshu\xingshu-prod.db`
- Linux: `${XDG_DATA_HOME:-$HOME/.local/share}/xingshu/xingshu-prod.db`

Resolution order: `--db <PATH>` > `XINGSHU_DB` > the build default shown above.
`XINGSHU_ENV=dev` uses the `dev/` subdirectory of the same data directory instead
(`XINGSHU_ENV` accepts only `dev` or `prod`).

`xs db path` only prints the resolved path; it does not create the database. The first
command that writes data creates it.

## 7. Back up your index

Stop using `xs`, then copy `xingshu-prod.db` together with the search sidecar files that sit
in the same directory (`xingshu-prod.db` plus `.xs-search-v2-*.sqlite`). Restoring is the
reverse: put the files back, start `xs` again. The index is rebuildable with
`xs index update --all-roots`, so old backups are never critical.

## 8. Uninstall

Delete the program (`xs.exe` or `~/.local/bin/xs`), then delete the data directory printed by
`xs db path` if you also want to drop the index. `xs` writes no registry keys and no files
outside those two locations.
