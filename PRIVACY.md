# Privacy notice — Mural for macOS

Last reviewed: 25 October 2026
Scope: Mural macOS 0.1.1 in this source tree. This notice describes the application code reviewed for the pilot; review it again for every release.

## Data handled on your Mac

Mural does not require an account. When you choose a wallpaper folder, it reads image files in that folder to show the library, previews, dimensions, and file sizes. It does not edit, move, or delete the original images.

Mural stores your selected folder paths, favorites, display selections, slideshow settings, and window preferences in `~/Library/Application Support/Mural/reglages.json`. It stores generated thumbnails in `~/Library/Caches/Mural/vignettes` and optional AVIF conversion copies in `~/Library/Caches/Mural/avif`. These files stay on your Mac unless you choose to share them.

The local diagnostic log is `~/Library/Logs/Mural/mural.log`. It can include folder paths, image names, search terms, display names and dimensions, and error details. Review it before sharing it with anyone; it may reveal information about your files or setup.

Mural uses macOS to apply a wallpaper to the display you choose and to open local files or folders in Finder. These actions are initiated by you in the app.

## Network and third parties

The Mural 0.1.1 application source reviewed for this notice contains no analytics, advertising, account service, telemetry, or application code that uploads your library, settings, logs, or images. Sparkle 2 checks for updates daily over HTTPS on GitHub (gaorfg-bit/homebrew-mural) and downloads an update when you request installation. GitHub receives standard connection information such as your IP address and the update request. System profiling is disabled. No library paths, images, favorites, or diagnostic logs are sent by this update integration. Update preferences are stored by Sparkle in macOS user defaults. macOS diagnostics or network activity by other software on your Mac are outside Mural's control.

The optional Ko-fi link is provided on the project README, not embedded in the desktop app. Opening it takes you to Ko-fi in your browser; Ko-fi's own privacy terms then apply. Supporting the project is optional and is not required to use Mural.

## Questions

For privacy questions or to report an issue, use the [official issue tracker](https://github.com/gaorfg-bit/homebrew-mural/issues/new). Issues are public: do not attach private images, unredacted logs, or personal file paths.
