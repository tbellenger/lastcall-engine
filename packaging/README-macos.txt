Last Call desktop player (macOS, unsigned)

This build is not signed or notarized. Gatekeeper quarantines files downloaded
from the internet and will refuse to open the app on the first launch.

Either right-click LastCall.app and choose Open, or clear the quarantine flag
in Terminal from the folder that contains the app:

    xattr -dr com.apple.quarantine LastCall.app

`xattr -d` clears one file. Use `-dr` so the flag is removed from the files
inside the bundle as well.

Double-click LastCall.app to play the bundled sample, The Gatehouse.
Arrow keys walk. Type a command and press Return. Enter dismisses narration.

    look
    look at the rim
    get key
    unlock door

To open a different package:

    LastCall.app/Contents/MacOS/advplay /path/to/game.advpak

With no package on the command line and no bundled sample, the player asks
for a .advpak file.
