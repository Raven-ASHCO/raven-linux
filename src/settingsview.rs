//! The seven Settings sections (Calm): their order, names, colours and
//! icons, and which way the content slides when the person moves between
//! them. Pure and GTK-free.

use crate::icons::Icon;
use crate::strings::tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    General,
    Appearance,
    Privacy,
    Lock,
    Network,
    Updates,
    About,
}

impl Section {
    pub const ALL: [Section; 7] = [
        Section::General,
        Section::Appearance,
        Section::Privacy,
        Section::Lock,
        Section::Network,
        Section::Updates,
        Section::About,
    ];

    /// The CSS suffix of its square icon (`sec-ic sec-general`, …), the same
    /// names as the tokens' `section*` colours.
    pub fn id(self) -> &'static str {
        match self {
            Section::General => "general",
            Section::Appearance => "appearance",
            Section::Privacy => "privacy",
            Section::Lock => "lock",
            Section::Network => "network",
            Section::Updates => "updates",
            Section::About => "about",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Section::General => tr("settings.general"),
            Section::Appearance => tr("settings.appearance"),
            Section::Privacy => tr("settings.privacy"),
            Section::Lock => tr("lock.section"),
            Section::Network => tr("settings.network"),
            Section::Updates => tr("updates.title"),
            Section::About => tr("net.about"),
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Section::General => Icon::Gear,
            Section::Appearance => Icon::Appearance,
            Section::Privacy => Icon::Shield,
            Section::Lock => Icon::Lock,
            Section::Network => Icon::Net,
            Section::Updates => Icon::Updates,
            Section::About => Icon::About,
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// Which way the new section's content comes in: from below when the
/// person moved down the list, from above when they moved up, none for the
/// first section shown or the same one again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slide {
    None,
    FromBelow,
    FromAbove,
}

pub fn slide(from: Option<Section>, to: Section) -> Slide {
    match from {
        None => Slide::None,
        Some(f) if f == to => Slide::None,
        Some(f) if to.index() > f.index() => Slide::FromBelow,
        Some(_) => Slide::FromAbove,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::SECTION_COLOURS;

    #[test]
    fn seven_sections_with_their_colours() {
        assert_eq!(Section::ALL.len(), 7);
        for (s, (id, _)) in Section::ALL.iter().zip(SECTION_COLOURS) {
            assert_eq!(s.id(), id, "the CSS has a colour for every section");
            assert!(!s.title().is_empty());
        }
        assert_eq!(Section::Lock.title(), "Lock");
        assert_eq!(Section::About.index(), 6);
    }

    #[test]
    fn the_content_slides_from_the_direction_of_travel() {
        assert_eq!(slide(None, Section::General), Slide::None);
        assert_eq!(
            slide(Some(Section::General), Section::Lock),
            Slide::FromBelow
        );
        assert_eq!(
            slide(Some(Section::About), Section::Privacy),
            Slide::FromAbove
        );
        assert_eq!(slide(Some(Section::Lock), Section::Lock), Slide::None);
    }
}
