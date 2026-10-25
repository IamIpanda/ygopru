//! The duel field state, rebuilt from the game message stream.
//!
//! This is the server-side counterpart of ygopro's `ClientField`: it consumes the same
//! `gm` messages the client does and keeps the board queryable without any rendering.
//! UI-only state (animations, selection panels, view-dependent flags) is dropped.

use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::AddAssign;

use slotmap::SlotMap;
use slotmap::new_key_type;

use crate::constants::*;
use crate::data::CardPosition;
use crate::data::InfoLocation;
use crate::data::QueryData;
use crate::data::UpdateCardInfo;
use crate::message::gm;

new_key_type! {
    /// Stable handle to a [`Card`] inside a [`Field`].
    ///
    /// That's a arena key for keeping the dual reloationships between card and card.
    pub struct CardKey;
}

/// A card tracked by the field.
pub struct Card {
    pub code: u32,
    // pub chain_code: u32, // UI only: code before it was hidden, used by flip/move animations and card buttons; removed.
    pub alias: u32,
    pub card_type: Type,
    pub level: u32,
    pub rank: u32,
    pub link: u32,
    pub attribute: Attribute,
    pub race: Race,
    pub attack: i32,
    pub defense: i32,
    pub base_attack: i32,
    pub base_defense: i32,
    pub left_scale: u32,
    pub right_scale: u32,
    pub link_marker: Linkmarkers,
    pub reason: Reason,
    pub owner: CorePlayer,
    pub controller: CorePlayer,
    pub location: Location,
    pub sequence: u8,
    pub position: Position,
    pub status: Status,
    pub card_hint_type: Option<CardHint>,
    pub card_hint_value: i32,
    // pub operation_parameter: u32, // UI only: selection opParam for select-sum panels; removed.
    pub overlay_target: Option<CardKey>,
    pub overlayed: Vec<CardKey>,
    pub equip_target: Option<CardKey>,
    pub equipped: HashSet<CardKey>,
    pub card_target: HashSet<CardKey>,
    pub owner_target: HashSet<CardKey>,
    pub counters: HashMap<u16, u16>,
    pub description_hints: HashMap<i32, i32>,
}

impl Default for Card {
    fn default() -> Self {
        Self {
            code: 0,
            alias: 0,
            card_type: Type::empty(),
            level: 0,
            rank: 0,
            link: 0,
            attribute: Attribute::empty(),
            race: Race::empty(),
            attack: 0,
            defense: 0,
            base_attack: 0,
            base_defense: 0,
            left_scale: 0,
            right_scale: 0,
            link_marker: Linkmarkers::empty(),
            reason: Reason::empty(),
            owner: CorePlayer::None,
            controller: CorePlayer::None,
            location: Location::Limbo,
            sequence: 0,
            position: Position::Any,
            status: Status::empty(),
            card_hint_type: None,
            card_hint_value: 0,
            overlay_target: None,
            overlayed: Vec::new(),
            equip_target: None,
            equipped: HashSet::new(),
            card_target: HashSet::new(),
            owner_target: HashSet::new(),
            counters: HashMap::new(),
            description_hints: HashMap::new(),
        }
    }
}

impl Card {
    fn set_code(&mut self, code: u32) {
        if self.code == code { return; }
        self.code = code;
    }

    fn clear(&mut self) {
        self.alias = 0;
        self.card_type = Type::empty();
        self.level = 0;
        self.rank = 0;
        self.race = Race::empty();
        self.attribute = Attribute::empty();
        self.attack = 0;
        self.defense = 0;
        self.base_attack = 0;
        self.base_defense = 0;
        self.left_scale = 0;
        self.right_scale = 0;
        self.link = 0;
        self.link_marker = Linkmarkers::empty();
        self.status = Status::empty();
        self.counters.clear();
    }
}

/// An effect in the chain.
pub struct ChainInfo {
    pub chain_card: Option<CardKey>,
    pub code: u32,
    pub description: i32,
    pub controller: CorePlayer,
    pub location: Location,
    pub sequence: u8,
    pub solved: bool,
    pub negated: bool,
    pub disabled: bool,
    pub target: HashSet<CardKey>,
}

impl Default for ChainInfo {
    fn default() -> Self {
        Self {
            chain_card: None,
            code: 0,
            description: 0,
            controller: CorePlayer::None,
            location: Location::Limbo,
            sequence: 0,
            solved: false,
            negated: false,
            disabled: false,
            target: HashSet::new(),
        }
    }
}

/// Duel-level state that survives a board reset.
pub struct DuelInfo {
    pub lp: [i32; 2],
    pub turn: i32,
    pub duel_rule: MasterRule,
}

impl Default for DuelInfo {
    fn default() -> Self {
        Self { lp: [0, 0], turn: 0, duel_rule: MasterRule::MasterRule2020 }
    }
}

impl DuelInfo {
    fn evolve(&mut self, message: &gm::Message) {
        match message {
            gm::Message::NewTurn(_) => self.turn += 1,
            gm::Message::Damage(damage) => self.lp[damage.player as usize] = (self.lp[damage.player as usize] - damage.value).max(0),
            gm::Message::Recover(recover) => self.lp[recover.player as usize] += recover.value,
            gm::Message::LPUpdate(update) => self.lp[update.player as usize] = update.lp,
            gm::Message::PayLPCost(cost) => self.lp[cost.player as usize] = (self.lp[cost.player as usize] - cost.cost).max(0),
            _ => (),
        }
    }
}

impl AddAssign<&gm::Message> for DuelInfo {
    fn add_assign(&mut self, message: &gm::Message) {
        self.evolve(message);
    }
}

/// The board as seen by one player.
///
/// Lists are ordered the way the core reports them, with the deck top at the end of
/// [`Field::deck`]; `mzone` and `szone` are fixed slots; overlay materials live in
/// [`Card::overlayed`] rather than in a zone.
///
/// The state is advanced only by [`Field::evolve`], so the public fields should be treated
/// as read-only snapshots.
pub struct Field {
    pub cards: SlotMap<CardKey, Card>,
    pub deck: [Vec<CardKey>; 2],
    pub hand: [Vec<CardKey>; 2],
    pub mzone: [[Option<CardKey>; 7]; 2],
    pub szone: [[Option<CardKey>; 8]; 2],
    pub grave: [Vec<CardKey>; 2],
    pub remove: [Vec<CardKey>; 2],
    pub extra: [Vec<CardKey>; 2],
    // pub overlay_cards: HashSet<CardKey>, // UI only: flat index of overlay cards for rendering/hover; removed (Card::overlayed covers it).
    pub extra_pendulum_count: [i32; 2],
    pub duel_info: DuelInfo,
    pub chains: Vec<ChainInfo>,
    pub current_chain: ChainInfo,
    // pub last_chain: bool, // UI only: animation pacing between CHAINED and CHAIN_SOLVING; removed.
    pub attacker: Option<CardKey>,
    pub attack_target: Option<CardKey>,
    pub disabled_field: u32,
    pub deck_reversed: bool,
    // pub tag_surrender: bool, // UI only: tag surrender button state; removed.
    // pub tag_teammate_surrender: bool, // UI only: tag teammate surrender hint; removed.
    // pub cannot_check_grave: bool, // UI only: puzzle hint blocks browsing own grave; removed.
    pub player_description_hints: [HashMap<i32, i32>; 2],
}
impl Default for Field {
    fn default() -> Self {
        Self {
            cards: SlotMap::with_key(),
            deck: [Vec::new(), Vec::new()],
            hand: [Vec::new(), Vec::new()],
            mzone: [[None; 7]; 2],
            szone: [[None; 8]; 2],
            grave: [Vec::new(), Vec::new()],
            remove: [Vec::new(), Vec::new()],
            extra: [Vec::new(), Vec::new()],
            extra_pendulum_count: [0, 0],
            duel_info: DuelInfo::default(),
            chains: Vec::new(),
            current_chain: ChainInfo::default(),
            attacker: None,
            attack_target: None,
            disabled_field: 0,
            deck_reversed: false,
            player_description_hints: [HashMap::new(), HashMap::new()],
        }
    }
}

impl Field {
    /// Creates an empty field.
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies a game message.
    pub fn evolve(&mut self, message: &gm::Message) {
        self.duel_info.evolve(message);
        match message {
            gm::Message::Start(start) => self.start(start),
            gm::Message::UpdateData(data) => self.update_data(data),
            gm::Message::UpdateCard(data) => self.update_card(data),
            gm::Message::ConfirmDecktop(confirm) => self.confirm_decktop(confirm),
            gm::Message::ConfirmCards(confirm) => self.confirm_cards(confirm),
            gm::Message::ShuffleDeck(shuffle) => self.shuffle_deck(shuffle.player),
            gm::Message::ShuffleHand(shuffle) => self.shuffle_hand(shuffle),
            gm::Message::SwapGraveDeck(swap) => self.swap_grave_deck(swap.player),
            gm::Message::ShuffleSetCard(shuffle) => self.shuffle_set_card(shuffle),
            gm::Message::ReverseDeck(_) => self.reverse_deck(),
            gm::Message::DeckTop(deck_top) => self.deck_top(deck_top),
            gm::Message::ShuffleExtra(shuffle) => self.shuffle_extra(shuffle),
            gm::Message::ConfirmExtraTop(confirm) => self.confirm_extra_top(confirm),
            gm::Message::Move(mov) => self.move_card(mov),
            gm::Message::PositionChange(change) => self.position_change(change),
            gm::Message::Swap(swap) => self.swap(swap),
            gm::Message::FieldDisabled(field_disabled) => self.disabled_field = field_disabled.disabled as u32,
            gm::Message::FlipSummoning(summon) => self.flip_summoning(summon),
            gm::Message::Chaining(chaining) => self.chaining(chaining),
            gm::Message::Chained(_) => {
                let current_chain = std::mem::take(&mut self.current_chain);
                self.chains.push(current_chain);
            }
            gm::Message::ChainSolving(solving) => self.chain_solving(solving.chain_index),
            gm::Message::ChainEnd(_) => self.chains.clear(),
            gm::Message::ChainNegated(negated) => self.chain_negated(negated.chain_index),
            gm::Message::ChainDisabled(disabled) => self.chain_disabled(disabled.chain_index),
            gm::Message::BecomeTarget(target) => self.become_target(target),
            gm::Message::Draw(draw) => self.draw(draw),
            gm::Message::Equip(equip) => self.equip(equip.position1, equip.position2),
            gm::Message::Unequip(unequip) => {
                let position = unequip.position;
                if let Some(key) = self.get_card(position.controller, position.location, position.sequence, position.overlay_sequence as i8) {
                    self.release_equip(key);
                }
            }
            gm::Message::CardTarget(target) => self.add_card_target(target.position1, target.position2),
            gm::Message::CancelTarget(target) => self.cancel_target(target.position1, target.position2),
            gm::Message::AddCounter(counter) => self.add_counter(&counter.position, counter.counter_type, counter.count),
            gm::Message::RemoveCounter(counter) => self.remove_counter(&counter.position, counter.counter_type, counter.count),
            gm::Message::Attack(attack) => self.attack(attack),
            gm::Message::Battle(battle) => self.battle(battle),
            gm::Message::AttackDisabled(_) => {
                self.attacker = None;
                self.attack_target = None;
            }
            gm::Message::CardHint(hint) => self.card_hint(hint),
            gm::Message::TagSwap(tag_swap) => self.tag_swap(tag_swap),
            gm::Message::ReloadField(reload) => self.reload_field(reload),
            gm::Message::PlayerHint(hint) => self.player_hint(hint),
            _ => (),
        }
    }

    fn clear(&mut self) {
        let duel_info = std::mem::take(&mut self.duel_info);
        *self = Self::default();
        self.duel_info = duel_info;
    }

    fn create_card(&mut self) -> CardKey {
        self.cards.insert(Card::default())
    }

    fn destroy_card(&mut self, key: CardKey) {
        let Some(card) = self.cards.remove(key) else { return };
        for equip in card.equipped {
            if let Some(equip_card) = self.cards.get_mut(equip) {
                equip_card.equip_target = None;
            }
        }
        if let Some(equip_target) = card.equip_target {
            if let Some(target) = self.cards.get_mut(equip_target) {
                target.equipped.remove(&key);
            }
        }
        for overlay in card.overlayed {
            self.clear_card_target(overlay);
            self.cards.remove(overlay);
        }
        if let Some(overlay_target) = card.overlay_target {
            if let Some(target) = self.cards.get_mut(overlay_target) {
                target.overlayed.retain(|overlay| *overlay != key);
            }
        }
        for target in card.card_target {
            if let Some(target_card) = self.cards.get_mut(target) {
                target_card.owner_target.remove(&key);
            }
        }
        for owner in card.owner_target {
            if let Some(owner_card) = self.cards.get_mut(owner) {
                owner_card.card_target.remove(&key);
            }
        }
    }

    fn get_card(&self, controller: CorePlayer, location: Location, sequence: u8, sub_sequence: i8) -> Option<CardKey> {
        let player = controller as usize;
        let key = match location.difference(Location::Overlay) {
            Location::Deck => self.deck[player].get(sequence as usize).copied(),
            Location::Hand => self.hand[player].get(sequence as usize).copied(),
            Location::MZone => self.mzone[player].get(sequence as usize).copied().flatten(),
            Location::SZone => self.szone[player].get(sequence as usize).copied().flatten(),
            Location::Grave => self.grave[player].get(sequence as usize).copied(),
            Location::Removed => self.remove[player].get(sequence as usize).copied(),
            Location::Extra => self.extra[player].get(sequence as usize).copied(),
            _ => None,
        };
        if !location.contains(Location::Overlay) {
            return key;
        }
        self.cards.get(key?)?.overlayed.get(sub_sequence.max(0) as usize).copied()
    }

    fn reset_sequence(cards: &mut SlotMap<CardKey, Card>, list: &[CardKey]) {
        for (sequence, key) in list.iter().enumerate() {
            if let Some(card) = cards.get_mut(*key) {
                card.sequence = sequence as u8;
            }
        }
    }

    fn remove_from_list(cards: &mut SlotMap<CardKey, Card>, list: &mut Vec<CardKey>, sequence: u8) -> Option<CardKey> {
        let sequence = sequence as usize;
        if sequence >= list.len() {
            return None;
        }
        let key = list.remove(sequence);
        Self::reset_sequence(cards, list);
        Some(key)
    }

    fn is_faceup_pendulum(card: &Card) -> bool {
        card.position.intersects(Position::Faceup) && card.card_type.intersects(Type::Pendulum)
    }

    fn add_card(&mut self, key: CardKey, controller: CorePlayer, location: Location, sequence: u8) {
        let player = controller as usize;
        if let Some(card) = self.cards.get_mut(key) {
            card.controller = controller;
            card.location = location;
            card.sequence = sequence;
        }
        match location {
            Location::Deck => {
                let index = (sequence as usize).min(self.deck[player].len());
                self.deck[player].insert(index, key);
                Self::reset_sequence(&mut self.cards, &self.deck[player]);
                self.with_card(key, |card| card.clear());
                self.clear_card_target(key);
            }
            Location::Hand => {
                self.hand[player].push(key);
                Self::reset_sequence(&mut self.cards, &self.hand[player]);
            }
            Location::MZone => {
                if let Some(slot) = self.mzone[player].get_mut(sequence as usize) {
                    *slot = Some(key);
                }
            }
            Location::SZone => {
                if let Some(slot) = self.szone[player].get_mut(sequence as usize) {
                    *slot = Some(key);
                }
            }
            Location::Grave => {
                self.grave[player].push(key);
                let sequence = self.grave[player].len() - 1;
                self.with_card(key, |card| card.sequence = sequence as u8);
            }
            Location::Removed => {
                self.remove[player].push(key);
                let sequence = self.remove[player].len() - 1;
                self.with_card(key, |card| card.sequence = sequence as u8);
            }
            Location::Extra => {
                let faceup = self.cards.get(key).map(Self::is_faceup_pendulum).unwrap_or(false);
                if self.extra_pendulum_count[player] == 0 || faceup {
                    self.extra[player].push(key);
                } else {
                    let pendulum_count = self.extra_pendulum_count[player].max(0) as usize;
                    let position = self.extra[player].len().saturating_sub(pendulum_count);
                    self.extra[player].insert(position, key);
                }
                Self::reset_sequence(&mut self.cards, &self.extra[player]);
                if faceup {
                    self.extra_pendulum_count[player] += 1;
                }
            }
            _ => (),
        }
    }

    fn remove_card(&mut self, controller: CorePlayer, location: Location, sequence: u8) -> Option<CardKey> {
        let player = controller as usize;
        let key = match location {
            Location::Deck => Self::remove_from_list(&mut self.cards, &mut self.deck[player], sequence),
            Location::Hand => Self::remove_from_list(&mut self.cards, &mut self.hand[player], sequence),
            Location::Grave => Self::remove_from_list(&mut self.cards, &mut self.grave[player], sequence),
            Location::Removed => Self::remove_from_list(&mut self.cards, &mut self.remove[player], sequence),
            Location::Extra => {
                let key = Self::remove_from_list(&mut self.cards, &mut self.extra[player], sequence);
                let faceup = key.and_then(|key| self.cards.get(key)).map(Self::is_faceup_pendulum).unwrap_or(false);
                if faceup {
                    self.extra_pendulum_count[player] -= 1;
                }
                key
            }
            Location::MZone => self.mzone[player].get_mut(sequence as usize).and_then(|slot| slot.take()),
            Location::SZone => self.szone[player].get_mut(sequence as usize).and_then(|slot| slot.take()),
            _ => None,
        };
        if let Some(key) = key {
            self.with_card(key, |card| card.location = Location::Limbo);
        }
        key
    }

    fn with_card(&mut self, key: CardKey, update: impl FnOnce(&mut Card)) {
        if let Some(card) = self.cards.get_mut(key) {
            update(card);
        }
    }

    fn clear_card_target(&mut self, key: CardKey) {
        let Some(card) = self.cards.get(key) else { return };
        let card_target = card.card_target.iter().copied().collect::<Vec<_>>();
        let owner_target = card.owner_target.iter().copied().collect::<Vec<_>>();
        for target in card_target {
            if let Some(target_card) = self.cards.get_mut(target) {
                target_card.owner_target.remove(&key);
            }
        }
        for owner in owner_target {
            if let Some(owner_card) = self.cards.get_mut(owner) {
                owner_card.card_target.remove(&key);
            }
        }
        if let Some(card) = self.cards.get_mut(key) {
            card.card_target.clear();
            card.owner_target.clear();
        }
    }

    fn release_equip(&mut self, key: CardKey) {
        let Some(equip_target) = self.cards.get(key).and_then(|card| card.equip_target) else { return };
        if let Some(target) = self.cards.get_mut(equip_target) {
            target.equipped.remove(&key);
        }
        self.with_card(key, |card| card.equip_target = None);
    }

    fn update_card_info(&mut self, key: CardKey, data: &UpdateCardInfo) {
        let UpdateCardInfo::Data(queries) = data else { return };
        for query in queries {
            match query {
                QueryData::Clear => self.with_card(key, |card| card.clear()),
                QueryData::Code(code) => {
                    if *code == 0 {
                        self.with_card(key, |card| card.clear());
                    }
                    self.with_card(key, |card| card.set_code(*code as u32));
                }
                QueryData::Position(position) => self.with_card(key, |card| card.position = position.position),
                QueryData::Alias(alias) => self.with_card(key, |card| card.alias = *alias as u32),
                QueryData::Type(card_type) => self.with_card(key, |card| card.card_type = *card_type),
                QueryData::Level(level) => self.with_card(key, |card| card.level = *level as u32),
                QueryData::Rank(rank) => self.with_card(key, |card| card.rank = *rank as u32),
                QueryData::Attribute(attribute) => self.with_card(key, |card| card.attribute = *attribute),
                QueryData::Race(race) => self.with_card(key, |card| card.race = *race),
                QueryData::Attack(attack) => self.with_card(key, |card| card.attack = *attack),
                QueryData::Defense(defense) => self.with_card(key, |card| card.defense = *defense),
                QueryData::BaseAttack(attack) => self.with_card(key, |card| card.base_attack = *attack),
                QueryData::BaseDefense(defense) => self.with_card(key, |card| card.base_defense = *defense),
                QueryData::Reason(reason) => self.with_card(key, |card| card.reason = *reason),
                QueryData::ReasonCard(_) => (),
                QueryData::EquipCard(position) => self.set_equip_card(key, position),
                QueryData::TargetCard(positions) => {
                    for position in positions {
                        self.add_card_target_query(key, position);
                    }
                }
                QueryData::OverlayCard(codes) => self.update_overlay_cards(key, codes),
                QueryData::Counters(counters) => self.with_card(key, |card| {
                    for (counter_type, count) in counters {
                        card.counters.insert(*counter_type, *count);
                    }
                }),
                QueryData::Owner(owner) => self.with_card(key, |card| card.owner = *owner),
                QueryData::Status(status) => self.with_card(key, |card| card.status = *status),
                QueryData::LeftScale(scale) => self.with_card(key, |card| card.left_scale = *scale as u32),
                QueryData::RightScale(scale) => self.with_card(key, |card| card.right_scale = *scale as u32),
                QueryData::Link(link, markers) => self.with_card(key, |card| {
                    card.link = *link as u32;
                    card.link_marker = *markers;
                }),
            }
        }
    }

    fn set_equip_card(&mut self, key: CardKey, position: &CardPosition<false, true, false>) {
        let Some(equip_target) = self.get_card(position.controller, position.location, position.sequence as u8, position.sub_sequence) else { return };
        self.with_card(key, |card| card.equip_target = Some(equip_target));
        if let Some(target) = self.cards.get_mut(equip_target) {
            target.equipped.insert(key);
        }
    }

    fn add_card_target_query(&mut self, key: CardKey, position: &CardPosition<false, true, false>) {
        let Some(target) = self.get_card(position.controller, position.location, position.sequence as u8, position.sub_sequence) else { return };
        self.with_card(key, |card| {
            card.card_target.insert(target);
        });
        if let Some(target_card) = self.cards.get_mut(target) {
            target_card.owner_target.insert(key);
        }
    }

    fn update_overlay_cards(&mut self, key: CardKey, codes: &[u32]) {
        let Some((owner, controller)) = self.cards.get(key).map(|card| (card.owner, card.controller)) else { return };
        for (sequence, code) in codes.iter().enumerate() {
            let overlay = match self.cards.get(key).and_then(|card| card.overlayed.get(sequence)).copied() {
                Some(overlay) => overlay,
                None => {
                    let overlay = self.create_card();
                    self.with_card(key, |card| card.overlayed.push(overlay));
                    self.with_card(overlay, |card| {
                        card.overlay_target = Some(key);
                        card.location = Location::Overlay;
                        card.sequence = sequence as u8;
                        card.owner = owner;
                        card.controller = controller;
                    });
                    overlay
                }
            };
            self.with_card(overlay, |card| card.set_code(*code));
        }
        let Some(removed) = self.cards.get_mut(key).map(|card| if card.overlayed.len() > codes.len() { card.overlayed.split_off(codes.len()) } else { Vec::new() }) else { return };
        for overlay in removed {
            self.clear_card_target(overlay);
            self.cards.remove(overlay);
        }
    }

    fn resequence_overlays(&mut self, key: CardKey) {
        let Some(overlays) = self.cards.get_mut(key).map(|card| std::mem::take(&mut card.overlayed)) else { return };
        for (sequence, overlay) in overlays.iter().enumerate() {
            self.with_card(*overlay, |card| card.sequence = sequence as u8);
        }
        self.with_card(key, |card| card.overlayed = overlays);
    }

    fn chain_mut(&mut self, chain_index: u8) -> Option<&mut ChainInfo> {
        let index = chain_index.checked_sub(1)?;
        self.chains.get_mut(index as usize)
    }

    fn initial(&mut self, player: CorePlayer, deck_count: u16, extra_count: u16) {
        for sequence in 0..deck_count {
            let key = self.create_card();
            self.with_card(key, |card| {
                card.owner = player;
                card.controller = player;
                card.location = Location::Deck;
                card.sequence = sequence as u8;
                card.position = Position::FacedownDefense;
            });
            self.deck[player as usize].push(key);
        }
        for sequence in 0..extra_count {
            let key = self.create_card();
            self.with_card(key, |card| {
                card.owner = player;
                card.controller = player;
                card.location = Location::Extra;
                card.sequence = sequence as u8;
                card.position = Position::FacedownDefense;
            });
            self.extra[player as usize].push(key);
        }
    }

    fn resize_zone(&mut self, player: CorePlayer, location: Location, count: usize) {
        loop {
            let length = match location {
                Location::Deck => self.deck[player as usize].len(),
                Location::Hand => self.hand[player as usize].len(),
                Location::Extra => self.extra[player as usize].len(),
                _ => return,
            };
            if length == count {
                return;
            }
            if length > count {
                let key = match location {
                    Location::Deck => self.deck[player as usize].pop(),
                    Location::Hand => self.hand[player as usize].pop(),
                    Location::Extra => self.extra[player as usize].pop(),
                    _ => return,
                };
                let Some(key) = key else { return };
                self.destroy_card(key);
                continue;
            }
            let key = self.create_card();
            self.with_card(key, |card| {
                card.owner = player;
                card.controller = player;
                card.location = location;
                card.sequence = length as u8;
            });
            match location {
                Location::Deck => self.deck[player as usize].push(key),
                Location::Hand => self.hand[player as usize].push(key),
                Location::Extra => self.extra[player as usize].push(key),
                _ => return,
            }
        }
    }

    fn load_player_field(&mut self, player: CorePlayer, field: &gm::PlayerField) {
        self.duel_info.lp[player as usize] = field.lp;
        for (sequence, slot) in field.mzone.iter().enumerate() {
            if slot.occupied == 0 {
                continue;
            }
            let key = self.create_card();
            self.add_card(key, player, Location::MZone, sequence as u8);
            self.with_card(key, |card| {
                card.owner = player;
                card.position = Position::from_bits_retain(slot.position);
            });
            for _ in 0..slot.xyz_count {
                let overlay = self.create_card();
                let overlay_sequence = match self.cards.get_mut(key) {
                    Some(card) => {
                        card.overlayed.push(overlay);
                        (card.overlayed.len() - 1) as u8
                    }
                    None => break,
                };
                self.with_card(overlay, |card| {
                    card.overlay_target = Some(key);
                    card.location = Location::Overlay;
                    card.sequence = overlay_sequence;
                    card.owner = player;
                    card.controller = player;
                });
            }
        }
        for (sequence, slot) in field.szone.iter().enumerate() {
            if slot.occupied == 0 {
                continue;
            }
            let key = self.create_card();
            self.add_card(key, player, Location::SZone, sequence as u8);
            self.with_card(key, |card| {
                card.owner = player;
                card.position = slot.position;
            });
        }
        for sequence in 0..field.main_count {
            let key = self.create_card();
            self.add_card(key, player, Location::Deck, sequence);
            self.with_card(key, |card| card.owner = player);
        }
        for sequence in 0..field.hand_count {
            let key = self.create_card();
            self.add_card(key, player, Location::Hand, sequence);
            self.with_card(key, |card| card.owner = player);
        }
        for sequence in 0..field.grave_count {
            let key = self.create_card();
            self.add_card(key, player, Location::Grave, sequence);
            self.with_card(key, |card| card.owner = player);
        }
        for sequence in 0..field.remove_count {
            let key = self.create_card();
            self.add_card(key, player, Location::Removed, sequence);
            self.with_card(key, |card| card.owner = player);
        }
        for sequence in 0..field.extra_count {
            let key = self.create_card();
            self.add_card(key, player, Location::Extra, sequence);
            self.with_card(key, |card| card.owner = player);
        }
        self.extra_pendulum_count[player as usize] = field.extra_p_count as i32;
    }

    fn start(&mut self, start: &gm::Start) {
        self.clear();
        self.duel_info.duel_rule = start.rule;
        self.duel_info.lp = [start.player1_lp, start.player2_lp];
        self.duel_info.turn = 0;
        self.cards = SlotMap::with_capacity_and_key(160);
        self.initial(CorePlayer::FirstAttackPlayer, start.player1_deck_count, start.player1_extra_count);
        self.initial(CorePlayer::SecondAttackPlayer, start.player2_deck_count, start.player2_extra_count);
    }

    fn update_data(&mut self, data: &gm::UpdateData) {
        for (sequence, info) in data.data.iter().enumerate() {
            let Some(key) = self.get_card(data.player, data.location, sequence as u8, 0) else { continue };
            self.update_card_info(key, info);
        }
    }

    fn update_card(&mut self, data: &gm::UpdateCard) {
        let position = &data.position;
        let Some(key) = self.get_card(position.controller, position.location, position.sequence as u8, 0) else { return };
        self.update_card_info(key, &data.data);
    }

    fn confirm_decktop(&mut self, message: &gm::ConfirmDecktop) {
        let player = message.controller as usize;
        for (offset, position) in message.codes.iter().enumerate() {
            if position.code.id() == 0 {
                continue;
            }
            let Some(index) = self.deck[player].len().checked_sub(1 + offset) else { break };
            let key = self.deck[player][index];
            self.with_card(key, |card| card.set_code(position.code.id()));
        }
    }

    fn confirm_cards(&mut self, message: &gm::ConfirmCards) {
        for position in &message.cards {
            if position.code.id() == 0 {
                continue;
            }
            let Some(key) = self.get_card(position.controller, position.location, position.sequence as u8, 0) else { continue };
            self.with_card(key, |card| card.set_code(position.code.id()));
        }
    }

    fn shuffle_deck(&mut self, player: CorePlayer) {
        for position in 0..self.deck[player as usize].len() {
            let key = self.deck[player as usize][position];
            self.with_card(key, |card| card.code = 0);
        }
    }

    fn shuffle_hand(&mut self, message: &gm::ShuffleHand) {
        let player = message.player as usize;
        for position in 0..self.hand[player].len() {
            let Some(code) = message.codes.get(position).copied() else { break };
            let key = self.hand[player][position];
            self.with_card(key, |card| {
                card.set_code(code);
                card.description_hints.clear();
            });
        }
    }

    fn swap_grave_deck(&mut self, player: CorePlayer) {
        std::mem::swap(&mut self.grave[player as usize], &mut self.deck[player as usize]);
        for position in 0..self.grave[player as usize].len() {
            let key = self.grave[player as usize][position];
            self.with_card(key, |card| card.location = Location::Grave);
        }
        let source = std::mem::take(&mut self.deck[player as usize]);
        let mut remaining = Vec::with_capacity(source.len());
        let mut sequence = 0u8;
        for key in source {
            let extra_deck = self.cards.get(key).map(|card| card.card_type.intersects(Type::Fusion | Type::Synchro | Type::Xyz | Type::Link)).unwrap_or(false);
            if extra_deck {
                self.with_card(key, |card| card.position = Position::Facedown);
                self.add_card(key, player, Location::Extra, 0);
            } else {
                let assigned = sequence;
                self.with_card(key, |card| {
                    card.location = Location::Deck;
                    card.sequence = assigned;
                });
                sequence += 1;
                remaining.push(key);
            }
        }
        self.deck[player as usize] = remaining;
    }

    fn shuffle_set_card(&mut self, message: &gm::ShuffleSetCard) {
        let mzone = message.location == Location::MZone;
        let mut selected = Vec::with_capacity(message.old_locations.len());
        for position in &message.old_locations {
            let key = if mzone {
                self.mzone[position.controller as usize].get(position.sequence as usize).copied().flatten()
            } else {
                self.szone[position.controller as usize].get(position.sequence as usize).copied().flatten()
            };
            if let Some(key) = key {
                self.with_card(key, |card| card.set_code(0));
            }
            selected.push(key);
        }
        for (selected_index, position) in message.new_locations.iter().enumerate() {
            if position.location == Location::Limbo {
                continue;
            }
            let Some(Some(key)) = selected.get(selected_index).copied() else { continue };
            let Some(sequence) = self.cards.get(key).map(|card| card.sequence) else { continue };
            let previous = if mzone {
                self.mzone[position.controller as usize].get_mut(position.sequence as usize).and_then(|slot| slot.take())
            } else {
                self.szone[position.controller as usize].get_mut(position.sequence as usize).and_then(|slot| slot.take())
            };
            if let Some(previous) = previous {
                self.with_card(previous, |card| card.sequence = sequence);
                if mzone {
                    if let Some(slot) = self.mzone[position.controller as usize].get_mut(sequence as usize) {
                        *slot = Some(previous);
                    }
                } else if let Some(slot) = self.szone[position.controller as usize].get_mut(sequence as usize) {
                    *slot = Some(previous);
                }
            }
            if mzone {
                if let Some(slot) = self.mzone[position.controller as usize].get_mut(position.sequence as usize) {
                    *slot = Some(key);
                }
            } else if let Some(slot) = self.szone[position.controller as usize].get_mut(position.sequence as usize) {
                *slot = Some(key);
            }
            self.with_card(key, |card| card.sequence = position.sequence);
        }
    }

    fn reverse_deck(&mut self) {
        for player in 0..2 {
            self.deck[player].reverse();
            Self::reset_sequence(&mut self.cards, &self.deck[player]);
        }
        self.deck_reversed = !self.deck_reversed;
    }

    fn deck_top(&mut self, message: &gm::DeckTop) {
        let player = message.player as usize;
        let Some(position) = self.deck[player].len().checked_sub(1 + message.sequence as usize) else { return };
        let key = self.deck[player][position];
        self.with_card(key, |card| card.set_code(message.code.id()));
    }

    fn shuffle_extra(&mut self, message: &gm::ShuffleExtra) {
        let player = message.player as usize;
        let mut codes = message.codes.iter();
        for position in 0..self.extra[player].len() {
            let key = self.extra[player][position];
            let faceup = self.cards.get(key).map(|card| card.position.intersects(Position::Faceup)).unwrap_or(false);
            if faceup {
                continue;
            }
            let Some(code) = codes.next().copied() else { break };
            self.with_card(key, |card| card.set_code(code));
        }
    }

    fn confirm_extra_top(&mut self, message: &gm::ConfirmExtraTop) {
        let player = message.player as usize;
        let faceup_count = self.extra_pendulum_count[player] as usize;
        for (offset, position) in message.selectable_cards.iter().enumerate() {
            if position.code.id() == 0 {
                continue;
            }
            let Some(index) = self.extra[player].len().checked_sub(1 + faceup_count + offset) else { break };
            let key = self.extra[player][index];
            self.with_card(key, |card| card.set_code(position.code.id()));
        }
    }

    fn move_card(&mut self, message: &gm::Move) {
        let previous = message.previous;
        let current = message.current;
        let code = message.code as u32;
        if previous.location == Location::Limbo {
            let key = self.create_card();
            self.with_card(key, |card| {
                card.owner = current.controller;
                card.position = current.position;
                card.set_code(code);
            });
            self.add_card(key, current.controller, current.location, current.sequence);
            return;
        }
        if current.location == Location::Limbo {
            let Some(key) = self.get_card(previous.controller, previous.location, previous.sequence, previous.overlay_sequence as i8) else { return };
            if code != 0 {
                self.with_card(key, |card| card.set_code(code));
            }
            self.clear_card_target(key);
            self.remove_card(previous.controller, previous.location, previous.sequence);
            self.destroy_card(key);
            return;
        }
        if !previous.location.contains(Location::Overlay) && !current.location.contains(Location::Overlay) {
            let Some(key) = self.get_card(previous.controller, previous.location, previous.sequence, 0) else { return };
            let card_code = self.cards.get(key).map(|card| card.code).unwrap_or(0);
            if code != 0 && card_code != code || current.location == Location::Extra && card_code != code {
                self.with_card(key, |card| card.set_code(code));
            }
            self.with_card(key, |card| {
                card.card_hint_type = None;
                card.card_hint_value = 0;
            });
            if previous.location.intersects(Location::OnField) && current.location != previous.location {
                self.with_card(key, |card| card.counters.clear());
            }
            if current.location != previous.location {
                self.clear_card_target(key);
                self.release_equip(key);
            }
            self.remove_card(previous.controller, previous.location, previous.sequence);
            self.with_card(key, |card| card.position = current.position);
            self.add_card(key, current.controller, current.location, current.sequence);
            return;
        }
        if !previous.location.contains(Location::Overlay) {
            let Some(key) = self.get_card(previous.controller, previous.location, previous.sequence, 0) else { return };
            if code != 0 {
                self.with_card(key, |card| card.set_code(code));
            }
            self.with_card(key, |card| {
                card.counters.clear();
                card.card_hint_type = None;
                card.card_hint_value = 0;
            });
            self.clear_card_target(key);
            self.release_equip(key);
            let overlay_location = current.location.difference(Location::Overlay);
            let Some(overlay_target) = self.get_card(current.controller, overlay_location, current.sequence, 0) else { return };
            self.remove_card(previous.controller, previous.location, previous.sequence);
            let sequence = match self.cards.get_mut(overlay_target) {
                Some(card) => {
                    card.overlayed.push(key);
                    (card.overlayed.len() - 1) as u8
                }
                None => return,
            };
            self.with_card(key, |card| {
                card.overlay_target = Some(overlay_target);
                card.location = Location::Overlay;
                card.sequence = sequence;
            });
            return;
        }
        if !current.location.contains(Location::Overlay) {
            let previous_location = previous.location.difference(Location::Overlay);
            let Some(overlay_target) = self.get_card(previous.controller, previous_location, previous.sequence, 0) else { return };
            let Some(key) = self.cards.get(overlay_target).and_then(|card| card.overlayed.get(previous.overlay_sequence as usize)).copied() else { return };
            self.with_card(overlay_target, |card| {
                card.overlayed.remove(previous.overlay_sequence as usize);
            });
            self.resequence_overlays(overlay_target);
            self.with_card(key, |card| {
                card.overlay_target = None;
                card.position = current.position;
            });
            self.add_card(key, current.controller, current.location, current.sequence);
            return;
        }
        let previous_location = previous.location.difference(Location::Overlay);
        let current_location = current.location.difference(Location::Overlay);
        let Some(previous_target) = self.get_card(previous.controller, previous_location, previous.sequence, 0) else { return };
        let Some(current_target) = self.get_card(current.controller, current_location, current.sequence, 0) else { return };
        let Some(key) = self.cards.get(previous_target).and_then(|card| card.overlayed.get(previous.overlay_sequence as usize)).copied() else { return };
        self.with_card(previous_target, |card| {
            card.overlayed.remove(previous.overlay_sequence as usize);
        });
        self.resequence_overlays(previous_target);
        let sequence = match self.cards.get_mut(current_target) {
            Some(card) => {
                card.overlayed.push(key);
                (card.overlayed.len() - 1) as u8
            }
            None => return,
        };
        self.with_card(key, |card| {
            card.overlay_target = Some(current_target);
            card.sequence = sequence;
        });
    }

    fn position_change(&mut self, message: &gm::PositionChange) {
        let Some(key) = self.get_card(message.controller, message.location, message.sequence, 0) else { return };
        if message.previous_position.intersects(Position::Faceup) && message.current_position.intersects(Position::Facedown) {
            self.with_card(key, |card| card.counters.clear());
            self.clear_card_target(key);
        }
        if message.card != 0 {
            self.with_card(key, |card| card.set_code(message.card));
        }
        self.with_card(key, |card| card.position = message.current_position);
    }

    fn swap(&mut self, message: &gm::Swap) {
        let Some(first) = self.get_card(message.position1.controller, message.position1.location, message.position1.sequence, message.position1.overlay_sequence as i8) else { return };
        let Some(second) = self.get_card(message.position2.controller, message.position2.location, message.position2.sequence, message.position2.overlay_sequence as i8) else { return };
        self.remove_card(message.position1.controller, message.position1.location, message.position1.sequence);
        self.remove_card(message.position2.controller, message.position2.location, message.position2.sequence);
        self.add_card(first, message.position2.controller, message.position2.location, message.position2.sequence);
        self.add_card(second, message.position1.controller, message.position1.location, message.position1.sequence);
    }

    fn flip_summoning(&mut self, message: &gm::FlipSummoning) {
        let position = message.position;
        let Some(key) = self.get_card(position.controller, position.location, position.sequence, position.overlay_sequence as i8) else { return };
        self.with_card(key, |card| {
            card.set_code(message.code);
            card.position = position.position;
        });
    }

    fn chaining(&mut self, message: &gm::Chaining) {
        let chain_card = self.get_card(message.previous.controller, message.previous.location, message.previous.sequence as u8, message.previous.sub_sequence);
        if let Some(key) = chain_card {
            self.with_card(key, |card| card.code = message.card);
        }
        self.current_chain = ChainInfo {
            chain_card,
            code: message.card,
            description: message.current.description,
            controller: message.current.controller,
            location: message.current.location,
            sequence: message.current.sequence as u8,
            solved: false,
            negated: false,
            disabled: false,
            target: HashSet::new(),
        };
    }

    fn chain_solving(&mut self, chain_index: u8) {
        if let Some(chain) = self.chain_mut(chain_index) {
            chain.solved = true;
        }
    }

    fn chain_negated(&mut self, chain_index: u8) {
        if let Some(chain) = self.chain_mut(chain_index) {
            chain.negated = true;
        }
    }

    fn chain_disabled(&mut self, chain_index: u8) {
        if let Some(chain) = self.chain_mut(chain_index) {
            chain.disabled = true;
        }
    }

    fn become_target(&mut self, message: &gm::BecomeTarget) {
        for position in &message.pcards {
            let Some(key) = self.get_card(position.controller, position.location, position.sequence, position.overlay_sequence as i8) else { continue };
            self.current_chain.target.insert(key);
        }
    }

    fn draw(&mut self, message: &gm::Draw) {
        let player = message.player as usize;
        for (offset, code) in message.codes.iter().enumerate() {
            let Some(position) = self.deck[player].len().checked_sub(1 + offset) else { break };
            let Some(key) = self.deck[player].get(position).copied() else { break };
            if !self.deck_reversed || code.id() != 0 {
                self.with_card(key, |card| card.set_code(code.id()));
            }
        }
        for _ in 0..message.codes.len() {
            let Some(key) = self.deck[player].pop() else { break };
            self.add_card(key, message.player, Location::Hand, 0);
        }
    }

    fn equip(&mut self, position1: InfoLocation, position2: InfoLocation) {
        let Some(equip_card) = self.get_card(position1.controller, position1.location, position1.sequence, position1.overlay_sequence as i8) else { return };
        let Some(target) = self.get_card(position2.controller, position2.location, position2.sequence, position2.overlay_sequence as i8) else { return };
        self.release_equip(equip_card);
        self.with_card(equip_card, |card| card.equip_target = Some(target));
        self.with_card(target, |card| {
            card.equipped.insert(equip_card);
        });
    }

    fn add_card_target(&mut self, position1: InfoLocation, position2: InfoLocation) {
        let Some(card) = self.get_card(position1.controller, position1.location, position1.sequence, position1.overlay_sequence as i8) else { return };
        let Some(target) = self.get_card(position2.controller, position2.location, position2.sequence, position2.overlay_sequence as i8) else { return };
        self.with_card(card, |card| {
            card.card_target.insert(target);
        });
        if let Some(target_card) = self.cards.get_mut(target) {
            target_card.owner_target.insert(card);
        }
    }

    fn cancel_target(&mut self, position1: InfoLocation, position2: InfoLocation) {
        let Some(card) = self.get_card(position1.controller, position1.location, position1.sequence, position1.overlay_sequence as i8) else { return };
        let Some(target) = self.get_card(position2.controller, position2.location, position2.sequence, position2.overlay_sequence as i8) else { return };
        self.with_card(card, |card| {
            card.card_target.remove(&target);
        });
        if let Some(target_card) = self.cards.get_mut(target) {
            target_card.owner_target.remove(&card);
        }
    }

    fn add_counter(&mut self, position: &CardPosition<false, false, false>, counter_type: u16, count: u16) {
        let Some(key) = self.get_card(position.controller, position.location, position.sequence as u8, 0) else { return };
        self.with_card(key, |card| {
            *card.counters.entry(counter_type).or_insert(0) += count;
        });
    }

    fn remove_counter(&mut self, position: &CardPosition<false, false, false>, counter_type: u16, count: u16) {
        let Some(key) = self.get_card(position.controller, position.location, position.sequence as u8, 0) else { return };
        self.with_card(key, |card| {
            if let Some(counter) = card.counters.get_mut(&counter_type) {
                *counter = counter.saturating_sub(count);
                if *counter == 0 {
                    card.counters.remove(&counter_type);
                }
            }
        });
    }

    fn attack(&mut self, message: &gm::Attack) {
        self.attacker = self.get_card(message.attacker.controller, message.attacker.location, message.attacker.sequence, message.attacker.overlay_sequence as i8);
        self.attack_target = if message.defenser.location == Location::Limbo {
            None
        } else {
            self.get_card(message.defenser.controller, message.defenser.location, message.defenser.sequence, message.defenser.overlay_sequence as i8)
        };
    }

    fn battle(&mut self, message: &gm::Battle) {
        let Some(attacker) = self.get_card(message.attacker.controller, message.attacker.location, message.attacker.sequence, message.attacker.overlay_sequence as i8) else { return };
        self.with_card(attacker, |card| {
            card.attack = message.attacker_attack;
            card.defense = message.attacker_defense;
        });
        let Some(defenser) = self.get_card(message.defenser.controller, message.defenser.location, message.defenser.sequence, message.defenser.overlay_sequence as i8) else { return };
        self.with_card(defenser, |card| {
            card.attack = message.defenser_attack;
            card.defense = message.defenser_defense;
        });
    }

    fn card_hint(&mut self, hint: &gm::CardHint) {
        let position = hint.position;
        let Some(key) = self.get_card(position.controller, position.location, position.sequence, position.overlay_sequence as i8) else { return };
        match hint.card_hint_type {
            CardHint::DescriptionAdd => self.with_card(key, |card| {
                *card.description_hints.entry(hint.value).or_insert(0) += 1;
            }),
            CardHint::DescriptionRemove => self.with_card(key, |card| {
                if let Some(count) = card.description_hints.get_mut(&hint.value) {
                    *count -= 1;
                    if *count == 0 {
                        card.description_hints.remove(&hint.value);
                    }
                }
            }),
            card_hint_type => self.with_card(key, |card| {
                card.card_hint_type = Some(card_hint_type);
                card.card_hint_value = hint.value;
            }),
        }
    }

    fn tag_swap(&mut self, message: &gm::TagSwap) {
        self.resize_zone(message.player, Location::Deck, message.main_count as usize);
        self.resize_zone(message.player, Location::Hand, message.hand_codes.len());
        self.resize_zone(message.player, Location::Extra, message.extra_codes.len());
        self.extra_pendulum_count[message.player as usize] = message.extra_p_count as i32;
        if let Some(key) = self.deck[message.player as usize].last().copied() {
            self.with_card(key, |card| card.code = message.top_code as u32);
        }
        for position in 0..self.hand[message.player as usize].len() {
            let Some(code) = message.hand_codes.get(position).copied() else { break };
            let key = self.hand[message.player as usize][position];
            self.with_card(key, |card| card.set_code(code.id()));
        }
        for position in 0..self.extra[message.player as usize].len() {
            let Some(code) = message.extra_codes.get(position).copied() else { break };
            let key = self.extra[message.player as usize][position];
            self.with_card(key, |card| card.set_code(code.id()));
        }
    }

    fn reload_field(&mut self, message: &gm::ReloadField) {
        self.clear();
        self.duel_info.duel_rule = message.duel_rule;
        self.load_player_field(CorePlayer::FirstAttackPlayer, &message.player1);
        self.load_player_field(CorePlayer::SecondAttackPlayer, &message.player2);
        for link in &message.chains {
            let chain_card = self.get_card(link.card_position.controller, link.card_position.location, link.card_position.sequence as u8, link.card_position.sub_sequence);
            if let Some(key) = chain_card {
                self.with_card(key, |card| card.code = link.code);
            }
            self.chains.push(ChainInfo {
                chain_card,
                code: link.code,
                description: link.description as i32,
                controller: link.controller,
                location: link.location,
                sequence: link.sequence as u8,
                solved: false,
                negated: false,
                disabled: false,
                target: HashSet::new(),
            });
        }
    }

    fn player_hint(&mut self, hint: &gm::PlayerHint) {
        let hints = &mut self.player_description_hints[hint.player as usize];
        match hint.player_hint_type {
            PlayerHint::DescriptionAdd => *hints.entry(hint.value).or_insert(0) += 1,
            PlayerHint::DescriptionRemove => {
                if let Some(count) = hints.get_mut(&hint.value) {
                    *count -= 1;
                    if *count == 0 {
                        hints.remove(&hint.value);
                    }
                }
            }
        }
    }
}

/// Same as [`Field::evolve`], for `field += &message`.
impl AddAssign<&gm::Message> for Field {
    fn add_assign(&mut self, message: &gm::Message) {
        self.evolve(message);
    }
}

#[cfg(test)]
mod tests {
    use std::alloc::GlobalAlloc;
    use std::alloc::Layout;
    use std::alloc::System;
    use std::mem::size_of;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
 
    use crate::constants::MasterRule;
    use crate::message::gm;

    use super::Card;
    use super::CardKey;
    use super::Field;

    static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

    struct Counting;

    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            ALLOCATED.fetch_sub(layout.size(), Ordering::Relaxed);
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            ALLOCATED.fetch_add(new_size, Ordering::Relaxed);
            ALLOCATED.fetch_sub(layout.size(), Ordering::Relaxed);
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static GLOBAL: Counting = Counting;

    fn start(deck: u16, extra: u16) -> gm::Message {
        gm::Message::Start(gm::Start {
            player_type: 0,
            rule: MasterRule::MasterRule2020,
            player1_lp: 8000,
            player2_lp: 8000,
            player1_deck_count: deck,
            player1_extra_count: extra,
            player2_deck_count: deck,
            player2_extra_count: extra,
        })
    }

    #[test]
    #[ignore]
    fn field_memory() {
        println!("Field={} Card={} CardKey={}", size_of::<Field>(), size_of::<Card>(), size_of::<CardKey>());
        let message = start(60, 15);
        let before = ALLOCATED.load(Ordering::Relaxed);
        let mut field = Field::new();
        field += &message;
        let heap = ALLOCATED.load(Ordering::Relaxed) - before;
        let total = size_of::<Field>() + heap;
        println!("cards={} capacity={} heap={} total={} ({:.1} KiB)", field.cards.len(), field.cards.capacity(), heap, total, total as f64 / 1024.0);
        std::hint::black_box(field);
    }
}
