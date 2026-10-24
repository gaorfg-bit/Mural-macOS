//! Mural — bibliotheque du portage macOS (Rust + AppKit via objc2).
//!
//! Modules :
//!   `ecrans`       ecrans connectes (equivalent de `mural/monitors.py`)
//!   `fond_ecran`   application du fond d'ecran (equivalent de `wallpaper_ops.py`)
//!   `preferences`  reglages persistants (equivalent de `mural/config.py`)
//!   `bibliotheque` liste des images et vignettes (equivalent de `thumbnails.py`)
//!   `suivi`        confirmation differee des demandes de fond (etat pur)
//!   `app`          interface AppKit (equivalent de `mural/app.py`)

pub mod app;
mod interface;
pub mod avif;
pub mod bibliotheque;
pub mod cache;
pub mod ecrans;
pub mod fond_ecran;
pub mod formats;
pub mod journal;
pub mod preferences;
pub mod reglages_panneau;
pub mod suivi;

pub mod diaporama;
