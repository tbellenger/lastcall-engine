Last Call desktop player (Linux x86_64)

Run the sample from this directory:

    ./advplay sample.advpak

Arrow keys walk. Type a command and press Enter. Enter dismisses narration.

    look
    look at the rim
    get key
    unlock door

With no argument, the player opens a file chooser for a .advpak. The chooser
uses the desktop portal. Playing from a path does not need a portal.

The binary is dynamically linked. A typical desktop needs X11 or Wayland,
libxkbcommon, ALSA (libasound), and GTK 3 for the file chooser. On Debian
or Ubuntu:

    sudo apt install libgtk-3-0 libx11-6 libxkbcommon0 libasound2

`advplay --dump-frame game.advpak` prints the boot framebuffer SHA-256 and
does not open a window. Opening a package by path does not show the chooser,
but the GTK library still has to be installed.
