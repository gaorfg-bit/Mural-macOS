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

<h1>Install</h2>

<p><strong>Step 1 — Install dependencies</strong></p>

<pre><code>xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh</code></pre>

<p><strong>Step 2 — Download and build</strong></p>

<pre><code>git clone https://github.com/gaorfg-bit/Mural-macOS
cd Mural-macOS
make bundle OFFLINE= SIGN=-
open Mural.app</code></pre>

<p><em>Mural is distributed as source for now: there is no public download yet, and the local
bundle is ad-hoc signed, not notarized. <code>OFFLINE=</code> lets Cargo fetch its dependencies
on a fresh machine; <code>SIGN=-</code> uses an ad-hoc signature when no local signing identity
is installed.</em></p>

<p><strong>Uninstall:</strong></p>

<pre><code>rm -rf Mural.app</code></pre>

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

<hr>

<p align="center">
  <img src="assets/Mural.png" width="900" alt="Mural screenshot">
</p>

<hr>

<p>macOS 13 or later, Intel or Apple silicon.</p>

<p>Your images, settings, caches and logs stay on your Mac — see <a href="PRIVACY.md">PRIVACY.md</a>.</p>

<p>GPL-3.0 — © 2026 GaoR</p>
