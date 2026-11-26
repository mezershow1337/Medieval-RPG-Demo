//! Мирные люди: жители города и торговцы. Они смертны, как все: их можно ранить и убить,
//! тело можно обыскать. А город отвечает на это по закону (см. `civil_server.rs`):
//! свидетели доносят страже, за голову назначают пеню, стража требует уплатить или идти
//! в карцер претории, иначе берётся за оружие.
//!
//! Детей, как и в больших ролевых играх, тронуть нельзя.

use crate::factions::Faction;
use crate::items::LootTableId;
use crate::level::Role;
use serde::{Deserialize, Serialize};

/// Кто это из мирных: житель (номер в `level::Layout::residents`) или торговец.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Civ {
    Resident(u16),
    Merchant(u16),
}

/// Сколько здоровья у мирного (без доспеха; солдаты в казарме крепче).
pub fn resident_hp(role: Role) -> f32 {
    match role {
        Role::Soldier | Role::Officer => 110.0,
        Role::Craftsman | Role::Sailor | Role::Farmer => 80.0,
        Role::Patient => 35.0,
        Role::Child => f32::INFINITY,
        _ => 65.0,
    }
}

pub const MERCHANT_HP: f32 = 80.0;

/// Можно ли ранить (детей — нет).
pub fn vulnerable(role: Role) -> bool {
    role != Role::Child
}

/// Чей это человек: за кого вступится закон и чья репутация пострадает.
pub fn role_faction(role: Role) -> Faction {
    match role {
        Role::Soldier | Role::Officer => Faction::Garrison,
        Role::Priest | Role::Monk | Role::Physician | Role::Patient => Faction::Church,
        Role::Noble => Faction::Exiles,
        Role::Sailor => Faction::Merchants,
        _ => Faction::Citizens,
    }
}

/// Что при нём (обыск тела).
pub fn role_loot(role: Role) -> LootTableId {
    match role {
        Role::Soldier | Role::Officer => LootTableId::Barracks,
        Role::Priest | Role::Monk => LootTableId::Church,
        Role::Physician => LootTableId::Apothecary,
        Role::Noble => LootTableId::Wealthy,
        Role::Craftsman => LootTableId::Workshop,
        Role::Cook => LootTableId::Kitchen,
        Role::Scholar | Role::Student => LootTableId::Library,
        _ => LootTableId::Household,
    }
}

/// Деньги в кошеле (в долях): от и до.
pub fn role_purse(role: Role) -> (u32, u32) {
    match role {
        Role::Noble => (400, 2400),
        Role::Officer | Role::Physician | Role::Priest => (120, 800),
        Role::Patient | Role::Child | Role::Servant | Role::Student | Role::Monk => (0, 40),
        _ => (20, 300),
    }
}

/// Преступления против людей. Пеня — в долях (медная монета = 10, серебряная = 100, золотая = 1000).
/// По римскому праву убийство — дело уголовное (lex Cornelia de sicariis), но в пограничном
/// Херсоне с его готскими и аланскими соседями дело нередко кончалось выкупом за кровь.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Violence {
    /// Нападение, побои (iniuria).
    Assault,
    /// Нападение на солдата гарнизона.
    AssaultGuard,
    /// Убийство.
    Murder,
    /// Убийство солдата.
    MurderGuard,
    /// Сопротивление страже.
    Resisting,
}

impl Violence {
    pub fn fine(self) -> u32 {
        match self {
            Violence::Assault => 400,
            Violence::AssaultGuard => 800,
            Violence::Murder => 7200,
            Violence::MurderGuard => 14400,
            Violence::Resisting => 400,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Violence::Assault => "нападение на горожанина",
            Violence::AssaultGuard => "нападение на солдата",
            Violence::Murder => "убийство",
            Violence::MurderGuard => "убийство солдата",
            Violence::Resisting => "сопротивление страже",
        }
    }

    /// Насколько падает репутация у фракции жертвы.
    pub fn rep(self) -> f32 {
        match self {
            Violence::Assault => -6.0,
            Violence::AssaultGuard => -8.0,
            Violence::Murder => -30.0,
            Violence::MurderGuard => -35.0,
            Violence::Resisting => -6.0,
        }
    }

    pub fn of(guard: bool, killed: bool) -> Violence {
        match (guard, killed) {
            (false, false) => Violence::Assault,
            (true, false) => Violence::AssaultGuard,
            (false, true) => Violence::Murder,
            (true, true) => Violence::MurderGuard,
        }
    }
}

/// Преступление, за которое могут донести страже.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crime {
    Violence(Violence),
    /// Кража (furtum manifestum: пойман с поличным — пеня в четверную цену вещи).
    Theft { value: u32 },
    /// Залез ночью в чужой дом.
    Trespass,
}

impl Crime {
    pub fn fine(self) -> u32 {
        match self {
            Crime::Violence(v) => v.fine(),
            Crime::Theft { value } => (value * 4).max(40),
            Crime::Trespass => 200,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Crime::Violence(v) => v.name(),
            Crime::Theft { .. } => "кража",
            Crime::Trespass => "вторжение в чужой дом",
        }
    }

    pub fn rep(self) -> f32 {
        match self {
            Crime::Violence(v) => v.rep(),
            Crime::Theft { value } => -(4.0 + (value as f32 / 60.0).min(10.0)),
            Crime::Trespass => -4.0,
        }
    }

    /// Насколько тяжкое (чтобы одно преступление против той же жертвы заменяло другое).
    pub fn weight(self) -> u32 {
        self.fine()
    }

    /// Насильственное (стража и жители берутся за оружие, а не просто кричат «держи вора»).
    pub fn violent(self) -> bool {
        matches!(self, Crime::Violence(_))
    }
}

/// Сколько секунд у вора, чтобы заставить свидетеля замолчать: потом тот добегает до стражи
/// (или кричит так, что слышит весь квартал). Время идёт, пока вы не рядом со свидетелем.
pub const WITNESS_SECS: f32 = 10.0;

/// Удастся ли уговорить стражника отпустить (0..1): от красноречия, тяжести вины и того,
/// как гарнизон к вам относится. Каждая неудачная попытка за разговор — хуже.
pub fn persuade_chance(eloquence: f32, fine: u32, garrison_rep: f32) -> f32 {
    let skill = eloquence.clamp(0.0, 100.0) / 100.0;
    let guilt = (fine as f32 / 7200.0).min(2.0);
    (0.12 + 0.6 * skill + garrison_rep.clamp(-100.0, 100.0) * 0.003 - 0.32 * guilt).clamp(0.03, 0.92)
}

/// Сколько секунд сидеть в карцере за такую пеню (время в игре идёт быстрее, чем в жизни).
pub fn jail_secs(fine: u32) -> f32 {
    (30.0 + fine as f32 / 120.0).min(240.0)
}

/// Ответ разыскиваемого страже.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LawAnswer {
    /// Уплатить пеню на месте.
    Pay,
    /// Сдаться: карцер претории, пеня списывается.
    Jail,
    /// Сопротивляться.
    Resist,
    /// Уговорить отпустить (навык «Красноречие»).
    Persuade,
}

/// Сервер -> клиент: закон и мирные люди.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum LawMessage {
    /// Короткое сообщение внизу экрана.
    Notice(String),
    /// Стражник остановил вас: за вами пеня. `persuade` — можно ли ещё попробовать уговорить
    /// (и красноречие для подсказки).
    Halt { fine: u32, crimes: Vec<String>, purse: u32, persuade: Option<u8> },
    /// Вас видели: сколько живых свидетелей и сколько секунд, пока они не донесли
    /// (`count == 0` — опасность миновала).
    Witnessed { count: u8, secs: f32 },
    /// Разговор со стражником окончен (ушли, уплатили, сдались или драка).
    HaltEnd,
    /// Вы в карцере ещё столько секунд.
    Jailed { secs: f32 },
    Released,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn murder_costs_more_than_assault() {
        assert!(Violence::Murder.fine() > Violence::Assault.fine() * 10);
        assert!(jail_secs(Violence::MurderGuard.fine()) <= 240.0);
        assert!(jail_secs(Violence::Assault.fine()) >= 30.0);
    }

    #[test]
    fn persuading_is_harder_for_worse_crimes() {
        let thief = persuade_chance(50.0, Crime::Theft { value: 30 }.fine(), 0.0);
        let killer = persuade_chance(50.0, Crime::Violence(Violence::Murder).fine(), 0.0);
        assert!(thief > killer + 0.2, "{thief} {killer}");
        assert!(persuade_chance(90.0, 120, 0.0) > persuade_chance(5.0, 120, 0.0));
        assert!(persuade_chance(100.0, 0, 100.0) <= 0.92 && persuade_chance(0.0, 100_000, -100.0) >= 0.03);
        assert!(Crime::Theft { value: 10 }.fine() >= 40);
    }

    #[test]
    fn children_are_untouchable() {
        assert!(!vulnerable(Role::Child));
        assert!(vulnerable(Role::Shopkeeper));
        assert_eq!(role_faction(Role::Soldier), Faction::Garrison);
    }
}
