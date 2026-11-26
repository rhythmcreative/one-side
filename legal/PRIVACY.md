# One-Side privacy

**In short: One-Side doesn't collect personal data or send it to its authors. It has no
accounts of its own, no servers, no analytics and no ads.**

## What's stored, and where
Everything stays on your Mac, in `~/Library/Application Support/One-Side`:
- Your bottles, their settings and the programs you install.
- An image cache (covers, icons and logos).
- Your settings, such as whether automatic updates are on.
- If you sign in to Epic Games or GOG: the session those stores give you, in the `irori`
  folder, which only your user can read. One-Side never sees your password, because you
  sign in on each store's own website. "Sign Out" deletes the session.

You can delete all of it by deleting that folder.

## What One-Side connects to
Only when it's needed for something you asked for, or for an update check, and without
sending any data about you:

| Service | What for | What it gets |
|---|---|---|
| Steam (store.steampowered.com, steamstatic.com) | Finding covers and images; downloading the Steam installer | The name of the game being looked up |
| GitHub (github.com, githubusercontent.com) | Downloading One-Side updates, the Mach9 and MetalSharp Wine (D3DMetal) engines, FSR 4, OptiScaler, winetricks and the game fixes, and checking for new versions of them | Only the request itself |
| Lutris (lutris.net) | Searching community installers | What you type in the search |
| Highball (gethighball.com) | Downloading its Mac compatibility database, the same for everyone | Only the request itself |
| Simple Icons (simpleicons.org) | Store logos | Only the request itself |
| Each store's or program's official website | Downloading its installer when you ask | Only the request itself |
| Epic Games and GOG | Only if you sign in: your library and your downloads | Whatever those stores need, under their own privacy policies |

Like any internet connection, these services see your IP address. Each one handles it
under its own privacy policy. Automatic updates can be turned off in Settings › Updates.

## Who's responsible
One-Side is free software that runs only on your computer. Its authors don't receive or
process any of your personal data, so they aren't data controllers under the General Data
Protection Regulation (GDPR).
