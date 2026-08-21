# How Sustena gets built

This is the plain-language version. No jargon, and where a technical word is
unavoidable it gets explained once and then used.

If you only ever read one line of this file: **work happens on a channel, and
`main` stays safe.**

---

## The idea in one picture

```
main    ────●──────────────────────●────────────────●────   always works
             \                    /                /
dev     ──────●────────●─────────●────────────────●──────   things being put together
               \      /           \              /
feat/orchie-ux  ●────●             \            /           one piece of work
feat/something-else                 ●──────────●            another, at the same time
```

Three kinds of line:

- **`main`** — the safe one. Whatever is on `main` works. Releases are cut from
  here. Nothing lands here until it has been checked.
- **`dev`** — the staging area. Finished pieces of work come here first and get
  to sit next to each other for a bit, in case two of them disagree.
- **working channels** — one per piece of work. Named for what they are:
  `feat/orchie-ux`, `fix/scroll-clipping`, `chore/devops-system`.

A channel is just a **separate copy of the project that you can break without
breaking anything else.** That is the whole point of it. You can have several
open at once and they do not interfere.

---

## Starting a new piece of work

Say you want to redo how Orchie looks. That is one piece of work, so it gets one
channel.

```powershell
cd C:\Users\DELL\dev\sustena
git checkout dev
git pull
git checkout -b feat/orchie-ux
```

Line by line: go to the project; switch to the staging line; get the latest of
it; make a new channel called `feat/orchie-ux` and switch onto it.

From here on, everything you change only exists on that channel. `main` is
untouched and still works.

**Naming.** `feat/` for something new, `fix/` for something broken,
`chore/` for housekeeping. After the slash, a couple of words with dashes.

---

## Saving your work as you go

```powershell
git add -A
git commit -m "orchie: bigger tap targets on the classify card"
git push
```

A **commit** is a save point with a note attached. Make them whenever a thought
is finished — they are cheap, and they are what lets you go back.

The first time you push a new channel, Git will ask you to be specific:

```powershell
git push -u origin feat/orchie-ux
```

Only the first time. After that plain `git push` knows where it goes.

---

## Finishing a piece of work

When the channel is done and working:

```powershell
git checkout dev
git pull
git merge feat/orchie-ux
git push
```

**Merge** means "fold this channel's work into that line". Now it is on `dev`,
sitting alongside anything else that landed recently.

When `dev` looks right, it goes to `main` the same way:

```powershell
git checkout main
git pull
git merge dev
git push
```

Then delete the finished channel, because it has served its purpose:

```powershell
git branch -d feat/orchie-ux
git push origin --delete feat/orchie-ux
```

### The safety net

Every push runs **CI** — a robot on GitHub that builds everything and runs every
test, on a machine that is not yours. If anything is broken it says so, on that
channel, before it can reach `main`.

Worth turning on in the GitHub settings (Settings → Branches → Add rule for
`main`): **require CI to pass before merging**, and **require the branch to be
up to date**. That makes the safety net compulsory rather than polite. Nobody
can then put something broken on `main`, including by accident.

---

## Cutting a new version

One command. It does everything.

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\release.ps1 -Bump minor -Notes "What changed, in a sentence."
```

`-Bump` is one of three words:

| word    | when                                          | 0.2.0 becomes |
| ------- | --------------------------------------------- | ------------- |
| `patch` | you fixed something, nothing new               | 0.2.1         |
| `minor` | something new, or something behaves differently | 0.3.0         |
| `major` | a deliberate break with what came before        | 1.0.0         |

While the version still starts with `0.`, `minor` is the usual one.

### What it does, in order

1. **Refuses** if the project is not on `main`, if you have unsaved changes, if
   that version was already released, or if **anything is broken**. It checks
   before it changes a single file, so a refusal costs you nothing.
2. Runs everything: both test suites, both linters, the typechecker, the
   frontend build.
3. Sets the new version number in all four places it lives, so they cannot
   drift apart.
4. Builds the **desktop app** (the `.msi` and `.exe` installers).
5. Builds the **phone app** (the `.apk`).
6. Writes the entry in `CHANGELOG.md`.
7. Saves and labels the release.
8. Copies both installers into `C:\Users\DELL\dev\sustena-installers\`, named
   with the version — so `Mycelium-Sustena-0.2.0-x64-setup.exe` rather than a
   file you have to guess about.

**If a build fails halfway, it puts every file back the way it was.** A failed
release leaves no mess.

Then, to publish it:

```powershell
git push origin main
git push origin v0.2.0
```

### Rehearsing without committing to it

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\release.ps1 -Bump minor -DryRun
```

Runs all the checks, tells you exactly what it *would* do, changes nothing.

---

## Where the version number lives

**One file: `VERSION` at the top of the project.** One line, nothing else.

Everything else copies from it — the engine, the app, the installer, the phone
app. The release script does the copying. You should never edit those by hand;
if they ever disagree, CI will notice and say so.

The phone version number is worked out automatically: `0.2.0` becomes
`versionCode 2000`. There is nothing to set.

---

## How updates reach the two apps

### Desktop (Mycelium) — the goal is "click to update"

Tauri, which the desktop app is built on, has an updater built in. The app
checks a small file on the internet, sees there is a newer version, and offers
to install it. **This is scaffolded but not switched on**, because it needs two
decisions that are yours to make:

1. **Where the update file lives.** GitHub Releases is the natural answer — you
   already have the repository, releases are free, and it is where the
   installers would go anyway.
2. **A signing key.** Updates are signed so the app will only install one that
   genuinely came from you. The key is generated once and **must be kept
   safe** — if it is lost, no already-installed copy will ever accept another
   update from you. It must never go into the repository.

When you have decided, `scripts/setup-updater.ps1` wires it up in one step.
Read that file's header first — it explains what it will do before it does it.

Until then, updating the desktop app means running the new installer, which
installs over the old one and keeps your data.

### Phone (Orchie) — honest options, none of them automatic yet

There is no app store involved, so there is no automatic update. Two real
choices, neither faked:

1. **Sideload each release.** The release script builds the `.apk` and names it
   with the version. Copy it to the phone and tap it; it installs over the old
   one and keeps your data. This is what happens today.
2. **A check-for-update prompt.** The app asks the same update file the desktop
   app would use, and if there is a newer version, offers a download link. You
   still tap to install — Android requires that — but you find out there is
   something to install without anyone telling you.

Option 2 depends on decision 1 above (where the file lives), so it waits on the
same answer.

**A note on this phone specifically:** installing over USB is blocked by
MIUI/HyperOS policy. Either turn on *Settings → Additional settings → Developer
options → Install via USB*, or copy the `.apk` to the phone and tap it in Files.

---

## When something goes wrong

**"I changed things and want to undo it."**

```powershell
git checkout -- .
```

Throws away every unsaved change on this channel. Committed work is untouched.

**"I am on the wrong channel."**

```powershell
git branch          # which one am I on? (the one with the *)
git checkout dev    # go to another
```

**"The release script refused."**

Read the line after `REFUSED:`. It says exactly what it wants and it has changed
nothing.

**"The build died with a memory error."**

Cold Rust builds on this machine run out of memory at full speed. Everything in
the release script already runs at half speed (`-j 2`) for that reason. If you
are running a build by hand, put `$env:CARGO_BUILD_JOBS = '2'` in front of it.

---

## The short version

```powershell
# start
git checkout dev; git pull; git checkout -b feat/thing

# work
git add -A; git commit -m "what I did"; git push

# finish
git checkout dev; git pull; git merge feat/thing; git push
git checkout main; git pull; git merge dev; git push

# release
.\scripts\release.ps1 -Bump minor -Notes "what changed"
git push origin main; git push origin v0.2.0
```
