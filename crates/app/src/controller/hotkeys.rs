//! Global shortcuts: which chord each slot wants, which one the system
//! actually granted, and what to tell the user when they differ.
//!
//! A chord can be registered once system-wide, so the slots are registered in
//! one fixed order and an earlier slot wins a chord two slots share. A slot's
//! own built-in default falls back to its Win+Alt form when it is taken; a
//! chord the user recorded never moves silently. A tool that is switched off
//! holds no chord.
use crate::controller::settings::{Edit, Tool};
use crate::ports::Hotkeys;
use powerkit_core::hotkey::{self, Hotkey};
use powerkit_core::settings::Preferences;

/// A global shortcut PowerKit registers, in registration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Pin,
    OpacityUp,
    OpacityDown,
    Ruler,
    AlwaysOn,
}
impl Slot {
    pub const COUNT: usize = 5;
    /// Registration order: an earlier slot wins a chord it shares with a later one.
    pub const ALL: [Self; Self::COUNT] = [
        Self::Pin,
        Self::OpacityUp,
        Self::OpacityDown,
        Self::Ruler,
        Self::AlwaysOn,
    ];
    fn index(self) -> usize {
        self as usize
    }
    /// The tool whose Home switch decides whether the slot is registered.
    pub fn tool(self) -> Tool {
        match self {
            Self::Pin | Self::OpacityUp | Self::OpacityDown => Tool::AlwaysTop,
            Self::Ruler => Tool::ScreenRuler,
            Self::AlwaysOn => Tool::AlwaysOn,
        }
    }
    pub fn default_chord(self) -> Hotkey {
        match self {
            Self::Pin => Hotkey::pin_default(),
            Self::OpacityUp => Hotkey::opacity_up_default(),
            Self::OpacityDown => Hotkey::opacity_down_default(),
            Self::Ruler => Hotkey::ruler_default(),
            Self::AlwaysOn => Hotkey::always_on_default(),
        }
    }
    /// The chord stored for this slot.
    pub fn chord(self, prefs: &Preferences) -> Hotkey {
        match self {
            Self::Pin => prefs.pin,
            Self::OpacityUp => prefs.opacity_up,
            Self::OpacityDown => prefs.opacity_down,
            Self::Ruler => prefs.ruler_hotkey,
            Self::AlwaysOn => prefs.always_on_hotkey,
        }
    }
    pub(crate) fn set_chord(self, prefs: &mut Preferences, chord: Hotkey) {
        match self {
            Self::Pin => prefs.pin = chord,
            Self::OpacityUp => prefs.opacity_up = chord,
            Self::OpacityDown => prefs.opacity_down = chord,
            Self::Ruler => prefs.ruler_hotkey = chord,
            Self::AlwaysOn => prefs.always_on_hotkey = chord,
        }
    }
}

fn stored(prefs: &Preferences) -> [(Slot, Hotkey); Slot::COUNT] {
    Slot::ALL.map(|slot| (slot, slot.chord(prefs)))
}

/// A recorded chord that another slot already holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rejected {
    pub slot: Slot,
    pub holder: Slot,
}

/// The edit that stores `chord` in `slot`, or the slot that already holds it.
/// Every slot counts, whether its tool is on or off, so switching a tool on
/// later cannot silently lose its shortcut.
pub fn record(prefs: &Preferences, slot: Slot, chord: Hotkey) -> Result<Edit, Rejected> {
    match hotkey::holder(&stored(prefs), slot, chord) {
        Some(holder) => Err(Rejected { slot, holder }),
        None => Ok(Edit::Hotkey(slot, chord)),
    }
}

/// What the system granted for each slot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registry {
    /// The chords last asked for; `None` for a slot whose tool is off.
    wanted: [Option<Hotkey>; Slot::COUNT],
    /// The chords that are registered, possibly a Win+Alt fallback.
    active: [Option<Hotkey>; Slot::COUNT],
}
impl Registry {
    /// A registry with nothing registered.
    pub const fn new() -> Self {
        Self {
            wanted: [None; Slot::COUNT],
            active: [None; Slot::COUNT],
        }
    }

    /// Registers what the preferences ask for. Nothing happens when every
    /// wanted chord is already active; otherwise all slots are released and
    /// registered again in order, so a chord that was taken is tried again and
    /// the winner of a shared chord does not depend on history.
    pub fn sync(&mut self, host: &mut impl Hotkeys, prefs: &Preferences) {
        let wanted = Slot::ALL.map(|slot| slot.tool().enabled(prefs).then(|| slot.chord(prefs)));
        let settled = wanted == self.wanted
            && Slot::ALL
                .iter()
                .all(|slot| wanted[slot.index()].is_none() || self.active(*slot).is_some());
        if settled {
            return;
        }
        self.pause(host);
        for slot in Slot::ALL {
            if let Some(chord) = wanted[slot.index()] {
                self.active[slot.index()] = chord
                    .register_with_fallback(chord == slot.default_chord(), |chord| {
                        host.register(slot, chord)
                    });
            }
        }
        self.wanted = wanted;
    }

    /// Releases every slot, as while a new chord is being recorded.
    pub fn pause(&mut self, host: &mut impl Hotkeys) {
        for slot in Slot::ALL {
            host.unregister(slot);
        }
        *self = Self::default();
    }

    /// The chord registered for `slot`; `None` when nothing is, so callers
    /// never show an inactive chord as active.
    pub fn active(&self, slot: Slot) -> Option<Hotkey> {
        self.active[slot.index()]
    }

    /// The text to show for `slot`: the registered chord, or the stored one
    /// while nothing is registered (tool off, recording in progress).
    /// `meta` is the platform's name for the meta key.
    pub fn label(&self, slot: Slot, prefs: &Preferences, meta: &str) -> String {
        self.active(slot)
            .unwrap_or_else(|| slot.chord(prefs))
            .label(meta)
    }
}

/// Why a slot's shortcut card carries a note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The chord just recorded belongs to `holder`; it was not stored.
    Duplicate { holder: Slot },
    /// The stored chord is also stored for `holder`, which registered first.
    Shared { holder: Slot },
    /// Another application holds the chord.
    Taken,
}

/// Notes for the shortcut cards: a refused recording, and stored chords an
/// enabled tool could not register. `quiet` suppresses the second kind while
/// nothing is registered on purpose (recording, self-test).
pub fn problems(
    prefs: &Preferences,
    registry: &Registry,
    rejected: Option<Rejected>,
    quiet: bool,
) -> Vec<(Slot, Problem)> {
    let mut notes = Vec::new();
    if let Some(Rejected { slot, holder }) = rejected {
        notes.push((slot, Problem::Duplicate { holder }));
    }
    if quiet {
        return notes;
    }
    let slots = stored(prefs);
    for (index, &(slot, chord)) in slots.iter().enumerate() {
        let noted = notes.iter().any(|(other, _)| *other == slot);
        if !slot.tool().enabled(prefs) || registry.active(slot).is_some() || noted {
            continue;
        }
        // Slots registered earlier win a shared chord (settings saved before
        // recording checked for duplicates).
        let earlier: Vec<_> = slots[..index]
            .iter()
            .copied()
            .filter(|(other, _)| other.tool().enabled(prefs))
            .collect();
        let problem = match hotkey::holder(&earlier, slot, chord) {
            Some(holder) => Problem::Shared { holder },
            None => Problem::Taken,
        };
        notes.push((slot, problem));
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::FakeHost;
    use powerkit_core::primitives::KeyCode;

    fn alt(chord: Hotkey) -> Hotkey {
        Hotkey::new(chord.meta, false, true, chord.shift, chord.key).unwrap()
    }
    fn off(tool: Tool) -> Preferences {
        let mut prefs = Preferences::default();
        Edit::ToggleTool(tool).apply(&mut prefs);
        prefs
    }

    #[test]
    fn every_slot_reads_and_writes_its_own_stored_chord() {
        let defaults = Preferences::default();
        let chord = Hotkey::new(false, true, true, false, KeyCode::SPACE).unwrap();
        for slot in Slot::ALL {
            assert_eq!(slot.chord(&defaults), slot.default_chord(), "{slot:?}");
            let mut prefs = defaults.clone();
            slot.set_chord(&mut prefs, chord);
            for other in Slot::ALL {
                let expected = if other == slot {
                    chord
                } else {
                    other.default_chord()
                };
                assert_eq!(other.chord(&prefs), expected, "{slot:?} {other:?}");
            }
        }
    }

    #[test]
    fn sync_registers_enabled_slots_in_order_and_nothing_twice() {
        let mut host = FakeHost::default();
        let mut registry = Registry::default();
        let prefs = Preferences::default();
        registry.sync(&mut host, &prefs);
        let order: Vec<Slot> = host.hotkey_log.iter().map(|(slot, _)| *slot).collect();
        assert_eq!(order, Slot::ALL);
        for slot in Slot::ALL {
            assert_eq!(registry.active(slot), Some(slot.default_chord()));
            assert_eq!(host.hotkeys.get(&slot), Some(&slot.default_chord()));
        }
        // Unchanged preferences touch nothing.
        registry.sync(&mut host, &prefs);
        assert_eq!(host.hotkey_log.len(), Slot::COUNT);
    }

    #[test]
    fn a_tool_that_is_off_holds_no_chord_and_gets_it_back_when_on() {
        let mut host = FakeHost::default();
        let mut registry = Registry::default();
        registry.sync(&mut host, &Preferences::default());
        let without_top = off(Tool::AlwaysTop);
        registry.sync(&mut host, &without_top);
        for slot in [Slot::Pin, Slot::OpacityUp, Slot::OpacityDown] {
            assert_eq!(registry.active(slot), None);
            assert!(!host.hotkeys.contains_key(&slot));
        }
        assert!(registry.active(Slot::Ruler).is_some());
        // Its stored chord is still what the card shows.
        assert_eq!(
            registry.label(Slot::Pin, &without_top, "Win"),
            Hotkey::pin_default().label("Win")
        );
        registry.sync(&mut host, &Preferences::default());
        assert_eq!(registry.active(Slot::Pin), Some(Hotkey::pin_default()));
    }

    #[test]
    fn a_taken_default_falls_back_to_win_alt_but_a_user_chord_does_not_move() {
        let mut host = FakeHost::default();
        host.taken_chords.push(Hotkey::pin_default());
        let mut registry = Registry::default();
        registry.sync(&mut host, &Preferences::default());
        let fallback = alt(Hotkey::pin_default());
        assert_eq!(registry.active(Slot::Pin), Some(fallback));
        assert_eq!(
            registry.label(Slot::Pin, &Preferences::default(), "Win"),
            fallback.label("Win")
        );
        // The same chord chosen for another slot is a user choice there.
        let mut prefs = Preferences::default();
        Slot::Ruler.set_chord(&mut prefs, Hotkey::pin_default());
        Slot::Pin.set_chord(&mut prefs, fallback);
        registry.sync(&mut host, &prefs);
        assert_eq!(registry.active(Slot::Ruler), None);
        assert_eq!(
            problems(&prefs, &registry, None, false),
            [(Slot::Ruler, Problem::Taken)]
        );
    }

    #[test]
    fn a_taken_chord_is_tried_again_on_the_next_sync() {
        let mut host = FakeHost::default();
        let mut prefs = Preferences::default();
        let chord = Hotkey::new(false, true, true, false, KeyCode::SPACE).unwrap();
        Slot::AlwaysOn.set_chord(&mut prefs, chord);
        host.taken_chords.push(chord);
        let mut registry = Registry::default();
        registry.sync(&mut host, &prefs);
        assert_eq!(registry.active(Slot::AlwaysOn), None);
        host.taken_chords.clear();
        registry.sync(&mut host, &prefs);
        assert_eq!(registry.active(Slot::AlwaysOn), Some(chord));
    }

    #[test]
    fn pausing_releases_every_slot_and_shows_the_stored_chords() {
        let mut host = FakeHost::default();
        let mut registry = Registry::default();
        let prefs = Preferences::default();
        registry.sync(&mut host, &prefs);
        registry.pause(&mut host);
        assert!(host.hotkeys.is_empty());
        for slot in Slot::ALL {
            assert_eq!(registry.active(slot), None);
            assert_eq!(
                registry.label(slot, &prefs, "Win"),
                slot.default_chord().label("Win")
            );
        }
        // A pause is not a failure to register.
        assert_eq!(problems(&prefs, &registry, None, true), []);
        registry.sync(&mut host, &prefs);
        assert_eq!(host.hotkeys.len(), Slot::COUNT);
    }

    #[test]
    fn recording_refuses_a_chord_another_slot_holds_even_when_its_tool_is_off() {
        let prefs = off(Tool::ScreenRuler);
        let taken = Hotkey::ruler_default();
        assert_eq!(
            record(&prefs, Slot::Pin, taken),
            Err(Rejected {
                slot: Slot::Pin,
                holder: Slot::Ruler
            })
        );
        // Recording a slot's own chord again is not a conflict.
        assert_eq!(
            record(&prefs, Slot::Ruler, taken),
            Ok(Edit::Hotkey(Slot::Ruler, taken))
        );
        let free = Hotkey::new(false, true, true, false, KeyCode::SPACE).unwrap();
        let mut stored = prefs.clone();
        record(&prefs, Slot::Pin, free).unwrap().apply(&mut stored);
        assert_eq!(stored.pin, free);
    }

    #[test]
    fn problems_name_a_refused_recording_a_shared_chord_and_a_taken_one() {
        let mut host = FakeHost::default();
        let mut registry = Registry::default();
        // A file saved before duplicates were refused: ruler shares the pin chord.
        let mut prefs = Preferences::default();
        Slot::Ruler.set_chord(&mut prefs, Hotkey::pin_default());
        let external = Hotkey::new(false, true, true, false, KeyCode::SPACE).unwrap();
        Slot::AlwaysOn.set_chord(&mut prefs, external);
        host.taken_chords.push(external);
        registry.sync(&mut host, &prefs);
        assert_eq!(
            problems(&prefs, &registry, None, false),
            [
                (Slot::Ruler, Problem::Shared { holder: Slot::Pin }),
                (Slot::AlwaysOn, Problem::Taken),
            ]
        );
        // A refused recording is reported once, in place of the slot's other note.
        let rejected = Rejected {
            slot: Slot::Ruler,
            holder: Slot::AlwaysOn,
        };
        assert_eq!(
            problems(&prefs, &registry, Some(rejected), false),
            [
                (
                    Slot::Ruler,
                    Problem::Duplicate {
                        holder: Slot::AlwaysOn
                    }
                ),
                (Slot::AlwaysOn, Problem::Taken),
            ]
        );
        assert_eq!(
            problems(&prefs, &registry, Some(rejected), true),
            [(
                Slot::Ruler,
                Problem::Duplicate {
                    holder: Slot::AlwaysOn
                }
            )]
        );
        // With AlwaysTop off its chord is free, so the ruler holds it alone.
        let mut without_top = prefs.clone();
        Edit::ToggleTool(Tool::AlwaysTop).apply(&mut without_top);
        registry.sync(&mut host, &without_top);
        assert_eq!(
            problems(&without_top, &registry, None, false),
            [(Slot::AlwaysOn, Problem::Taken)]
        );
    }
}
