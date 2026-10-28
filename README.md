<p align="center">
  <img src="assets/logo.png" width="850" alt="Mural logo">
</p>

<p align="center">
  <strong>Free wallpaper library manager for macOS.</strong><br>
  Per-display wallpapers. Slideshow. Native AppKit.
</p>

<p align="center">
<a href='https://ko-fi.com/M4M51RKN7V' target='_blank'><img height='36' style='border:0px;height:36px;' src='https://storage.ko-fi.com/cdn/kofi6.png?v=6' border='0' alt='Buy Me a Coffee at ko-fi.com' /></a>
</p>

<p>Mural is a local-first wallpaper library: point it at a folder, browse and preview your
images, keep favorites, and organize a slideshow for each display. It never modifies,
moves or deletes your files.</p>

<hr>

<h2>Features</h2>

<ul>
  <li>Independent wallpaper per display</li>
  <li>No overwriting — your images are never modified, moved or deleted</li>
  <li>Library with search, sort, preview and folder favorites</li>
  <li>Slideshow mode with per-display image selections</li>
  <li>Optional AVIF reading and conversion</li>
  <li>Native AppKit interface, no Electron</li>
</ul>

<h2>Formats</h2>

<p>PNG, JPEG, BMP and WebP. AVIF is read natively when macOS ImageIO provides the decoder.
The optional AVIF conversion writes a lossy copy to the app cache and never modifies the original.</p>

<hr>

<p align="center">
  <img src="assets/Mural.png" width="900" alt="Mural screenshot">
</p>

https://github.com/user-attachments/assets/3da52a0b-04a9-4d56-9e5b-abd66f01ace4

<hr>

<h2>Install</h2>

<p><strong>Homebrew</strong></p>

<pre><code>brew install --cask gaorfg-bit/mural/mural-macos</code></pre>

<p><em>Apple silicon; macOS 13 is the declared minimum, with compatibility on older
macOS versions still being tested. Version 0.1.1 is a beta and is not notarized.
If macOS blocks its first launch, read
<a href="https://support.apple.com/102445">Apple’s instructions</a> before deciding
whether to allow it in System Settings → Privacy &amp; Security → Open Anyway.
Read the <a href="https://github.com/gaorfg-bit/homebrew-mural/releases/tag/v0.1.1">release notes and installation instructions</a> before trying the beta.</em></p>

<p><strong>Build from source</strong></p>

<p><strong>Step 1 — Install dependencies</strong></p>

<pre><code>xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh</code></pre>

<p><strong>Step 2 — Download and build</strong></p>

<pre><code>git clone https://github.com/gaorfg-bit/Mural-macOS
cd Mural-macOS
make sparkle
make bundle OFFLINE= SIGN=-
open Mural.app</code></pre>

<p>Rust 1.88 or later is required. Tests are verified with Rust 1.88.0 and 1.98.1.</p>

<p><em><code>OFFLINE=</code> lets Cargo fetch its dependencies on a fresh machine;
<code>SIGN=-</code> uses an ad-hoc signature when no local signing identity is installed.</em></p>

<p><strong>Uninstall:</strong></p>

<pre><code>brew uninstall --cask mural-macos
rm -rf Mural.app   # source build only</code></pre>

<hr>

<p>macOS 13 or later. The signed 0.1.0 build is Apple silicon only.</p>

<p>Your images, settings, caches and logs stay on your Mac — see <a href="PRIVACY.md">PRIVACY.md</a>.</p>

<p>GPL-3.0 — © 2026 GaoR</p>

## Updates

Starting with 0.1.1, Mural checks for updates daily using Sparkle and offers to install them.
Use **Mural → Check for Updates…** for a manual check. Update archives are signed.
The existing 0.1.0 needs one manual upgrade to gain the built-in updater.

For a source build, run `make sparkle` once to download and verify Sparkle 2.10.0,
then `make bundle`. Cargo builds and subsequent bundle builds remain offline.
See [update distribution](docs/MISES-A-JOUR.md) for release preparation.
