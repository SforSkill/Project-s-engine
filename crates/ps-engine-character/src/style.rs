//! Team palettes, quality levels and the player's character settings.
//!
//! Every visual parameter of the liquid look lives here (`LiquidStyle`), so
//! art direction is tuned in one place. Palettes only change hue: all of
//! them keep a similar luminance and the same contour strength, so no team
//! colour is easier to see than another.

use bevy::color::{Color, LinearRgba, Srgba};
use engine_core::Team;
use serde::{Deserialize, Serialize};

/// One liquid paint: body colour, shadow/depth colour, contour and highlight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub primary: Srgba,
    pub secondary: Srgba,
    pub contour: Srgba,
}

const fn hex(v: u32) -> Srgba {
    Srgba::rgb(
        ((v >> 16) & 0xff) as f32 / 255.0,
        ((v >> 8) & 0xff) as f32 / 255.0,
        (v & 0xff) as f32 / 255.0,
    )
}

/// Named palettes selectable for allies and enemies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaletteId {
    Blue,
    Red,
    Emerald,
    Violet,
    Yellow,
    Cyan,
    Orange,
    White,
    Black,
}

impl PaletteId {
    pub const ALL: [PaletteId; 9] = [
        PaletteId::Blue,
        PaletteId::Red,
        PaletteId::Emerald,
        PaletteId::Violet,
        PaletteId::Yellow,
        PaletteId::Cyan,
        PaletteId::Orange,
        PaletteId::White,
        PaletteId::Black,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PaletteId::Blue => "Bleu",
            PaletteId::Red => "Rouge",
            PaletteId::Emerald => "Émeraude",
            PaletteId::Violet => "Violet",
            PaletteId::Yellow => "Jaune",
            PaletteId::Cyan => "Cyan",
            PaletteId::Orange => "Orange",
            PaletteId::White => "Blanc",
            PaletteId::Black => "Noir",
        }
    }

    pub fn palette(self) -> Palette {
        let (p, s, c) = match self {
            // Team A / Team B values of the art direction brief.
            PaletteId::Blue => (0x2563EB, 0x1D4ED8, 0x93C5FD),
            PaletteId::Red => (0xEF4444, 0xB91C1C, 0xFCA5A5),
            PaletteId::Emerald => (0x10B981, 0x047857, 0x6EE7B7),
            PaletteId::Violet => (0x8B5CF6, 0x6D28D9, 0xC4B5FD),
            PaletteId::Yellow => (0xEAB308, 0xA16207, 0xFDE68A),
            PaletteId::Cyan => (0x06B6D4, 0x0E7490, 0xA5F3FC),
            PaletteId::Orange => (0xF97316, 0xC2410C, 0xFDBA74),
            // White and black keep a contrasting contour so they never melt
            // into bright or dark maps.
            PaletteId::White => (0xE5E7EB, 0x9CA3AF, 0x60A5FA),
            PaletteId::Black => (0x27272A, 0x09090B, 0xA1A1AA),
        };
        Palette {
            primary: hex(p),
            secondary: hex(s),
            contour: hex(c),
        }
    }

    /// Colour for HUD elements (names, kill feed) matching the body.
    pub fn ui_rgb(self) -> [u8; 3] {
        let c = self.palette().contour.mix(&self.palette().primary, 0.45);
        [
            (c.red * 255.0) as u8,
            (c.green * 255.0) as u8,
            (c.blue * 255.0) as u8,
        ]
    }
}

trait Mix {
    fn mix(&self, other: &Self, t: f32) -> Self;
}

impl Mix for Srgba {
    fn mix(&self, o: &Srgba, t: f32) -> Srgba {
        Srgba::rgb(
            self.red + (o.red - self.red) * t,
            self.green + (o.green - self.green) * t,
            self.blue + (o.blue - self.blue) * t,
        )
    }
}

/// How players are coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorMode {
    /// Each side keeps its colour (defense = team A, attack = team B).
    Sides,
    /// Allies and enemies use the player's chosen colours, whatever the side.
    Relative,
}

/// Cosmetic quality. Every level shows the same silhouette, size and
/// contour: only secondary detail changes, never competitive visibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Quality {
    /// Facets and flat colours, no particles.
    Low,
    /// Plus contours (rim) and highlights, a few drops.
    Medium,
    /// Plus internal ripples and full particles.
    High,
    /// Plus facet edges, wobble and secondary motion.
    Ultra,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::Low, Quality::Medium, Quality::High, Quality::Ultra];

    pub fn label(self) -> &'static str {
        match self {
            Quality::Low => "Faible",
            Quality::Medium => "Moyenne",
            Quality::High => "Élevée",
            Quality::Ultra => "Ultra",
        }
    }

    pub fn level(self) -> u32 {
        self as u32
    }

    /// Simultaneous liquid droplets.
    pub fn max_droplets(self) -> usize {
        match self {
            Quality::Low => 0,
            Quality::Medium => 48,
            Quality::High => 128,
            Quality::Ultra => 256,
        }
    }

    /// Distance (m) beyond which a character switches to the next LOD.
    pub fn lod_distances(self) -> (f32, f32) {
        match self {
            Quality::Low => (8.0, 20.0),
            Quality::Medium => (12.0, 28.0),
            Quality::High => (16.0, 36.0),
            Quality::Ultra => (22.0, 48.0),
        }
    }
}

/// Saved in `settings.toml` under `[characters]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterSettings {
    pub quality: Quality,
    pub mode: ColorMode,
    /// `Sides` mode.
    pub defense: PaletteId,
    pub attack: PaletteId,
    /// `Relative` mode.
    pub allies: PaletteId,
    pub enemies: PaletteId,
}

impl Default for CharacterSettings {
    fn default() -> Self {
        Self {
            quality: Quality::High,
            mode: ColorMode::Sides,
            defense: PaletteId::Blue,
            attack: PaletteId::Red,
            allies: PaletteId::Blue,
            enemies: PaletteId::Orange,
        }
    }
}

impl CharacterSettings {
    /// Palette of a player of `team` seen by a viewer of `viewer` team.
    /// Two different teams never get the same palette: if the player picked
    /// the same one twice, the second side falls back to a distinct colour.
    pub fn palette_for(&self, team: Team, viewer: Team) -> PaletteId {
        let (a, b) = self.pair();
        match self.mode {
            ColorMode::Sides => match team {
                Team::Defense => a,
                Team::Attack => b,
                Team::Spectator => PaletteId::White,
            },
            ColorMode::Relative => {
                if team == Team::Spectator {
                    PaletteId::White
                } else if viewer == Team::Spectator {
                    // A spectator has no allies: fall back to sides.
                    if team == Team::Defense { a } else { b }
                } else if team == viewer {
                    a
                } else {
                    b
                }
            }
        }
    }

    /// Same as [`Self::palette_for`], aware of free-for-all modes
    /// (deathmatch): there, in `Relative` mode, every other player is an
    /// enemy whatever their side.
    pub fn palette_in(&self, team: Team, viewer: Team, free_for_all: bool) -> PaletteId {
        if free_for_all && self.mode == ColorMode::Relative && team != Team::Spectator {
            return self.pair().1;
        }
        self.palette_for(team, viewer)
    }

    /// (first, second) palettes of the current mode, guaranteed distinct.
    pub fn pair(&self) -> (PaletteId, PaletteId) {
        let (a, b) = match self.mode {
            ColorMode::Sides => (self.defense, self.attack),
            ColorMode::Relative => (self.allies, self.enemies),
        };
        if a != b {
            return (a, b);
        }
        let alt = if a == PaletteId::Red {
            PaletteId::Blue
        } else {
            PaletteId::Red
        };
        (a, alt)
    }
}

/// Every shader parameter of the liquid look, in one place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidStyle {
    /// Contour (Fresnel rim) strength, 0..1.
    pub contour_intensity: f32,
    /// Stylised highlight strength, 0..1.
    pub highlight: f32,
    /// Perceptual roughness: highlight size.
    pub roughness: f32,
    /// Body opacity. Kept at 1 for players: a translucent enemy would be
    /// harder to see on some backgrounds.
    pub opacity: f32,
    /// Internal ripple speed (cycles per second).
    pub ripple_speed: f32,
    /// Ripple / wobble amplitude, 0..1.
    pub deformation: f32,
    /// Idle drips per second per character.
    pub drip_rate: f32,
    /// Droplets per hit splash at full quality.
    pub splash: f32,
    /// Number of toon light bands.
    pub bands: f32,
}

impl Default for LiquidStyle {
    fn default() -> Self {
        Self {
            contour_intensity: 0.55,
            highlight: 0.6,
            roughness: 0.32,
            opacity: 1.0,
            ripple_speed: 0.35,
            deformation: 0.5,
            drip_rate: 0.25,
            splash: 10.0,
            bands: 3.0,
        }
    }
}

/// Linear colour helper for the shader uniform.
pub fn linear(c: Srgba) -> LinearRgba {
    Color::Srgba(c).to_linear()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Srgba) -> f32 {
        let l = linear(c);
        0.2126 * l.red + 0.7152 * l.green + 0.0722 * l.blue
    }

    #[test]
    fn brief_colours_are_exact() {
        let b = PaletteId::Blue.palette();
        assert_eq!(b.primary.to_hex(), "#2563EB");
        assert_eq!(b.secondary.to_hex(), "#1D4ED8");
        assert_eq!(b.contour.to_hex(), "#93C5FD");
        let r = PaletteId::Red.palette();
        assert_eq!(r.primary.to_hex(), "#EF4444");
        assert_eq!(r.secondary.to_hex(), "#B91C1C");
        assert_eq!(r.contour.to_hex(), "#FCA5A5");
    }

    #[test]
    fn teams_never_share_a_palette() {
        let mut s = CharacterSettings::default();
        for mode in [ColorMode::Sides, ColorMode::Relative] {
            s.mode = mode;
            for a in PaletteId::ALL {
                for b in PaletteId::ALL {
                    s.defense = a;
                    s.attack = b;
                    s.allies = a;
                    s.enemies = b;
                    for viewer in [Team::Attack, Team::Defense, Team::Spectator] {
                        assert_ne!(
                            s.palette_for(Team::Attack, viewer),
                            s.palette_for(Team::Defense, viewer)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn relative_mode_follows_viewer() {
        let s = CharacterSettings {
            mode: ColorMode::Relative,
            ..Default::default()
        };
        assert_eq!(s.palette_for(Team::Attack, Team::Attack), s.allies);
        assert_eq!(s.palette_for(Team::Defense, Team::Attack), s.enemies);
        assert_eq!(s.palette_for(Team::Defense, Team::Defense), s.allies);
        // Deathmatch: nobody is an ally.
        assert_eq!(s.palette_in(Team::Attack, Team::Attack, true), s.enemies);
        assert_eq!(s.palette_in(Team::Defense, Team::Attack, true), s.enemies);
    }

    #[test]
    fn contours_stand_out_from_bodies() {
        // The rim must differ from the body in luminance for every palette,
        // so the silhouette reads on any background.
        for p in PaletteId::ALL {
            let pal = p.palette();
            let d = (luminance(pal.contour) - luminance(pal.primary)).abs();
            assert!(d > 0.08, "{p:?}: contour contrast {d}");
        }
    }
}
