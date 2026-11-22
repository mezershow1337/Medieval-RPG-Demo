//! Инвентарь: сетка (предметы занимают несколько клеток) + слоты экипировки.
//!
//! Чистая логика без графики и сети. Одинаково устроены инвентарь игрока и тело,
//! которое можно обыскать: у тела тоже есть сетка (сумка) и надетые вещи.

use crate::items::{EquipSlot, Item, ItemUid, StatTotals};
use serde::{Deserialize, Serialize};

pub const PLAYER_GRID: (u8, u8) = (10, 6);
pub const CORPSE_GRID: (u8, u8) = (8, 5);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Placed {
    pub item: Item,
    pub x: u8,
    pub y: u8,
}

impl Placed {
    fn overlaps(&self, x: u8, y: u8, w: u8, h: u8) -> bool {
        let (iw, ih) = self.item.size();
        self.x < x + w && x < self.x + iw && self.y < y + h && y < self.y + ih
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Grid {
    pub w: u8,
    pub h: u8,
    pub items: Vec<Placed>,
}

impl Grid {
    pub fn new(w: u8, h: u8) -> Self {
        Self { w, h, items: Vec::new() }
    }

    fn in_bounds(&self, x: u8, y: u8, w: u8, h: u8) -> bool {
        x as u16 + w as u16 <= self.w as u16 && y as u16 + h as u16 <= self.h as u16
    }

    /// Индексы предметов, которые пересекаются с прямоугольником.
    fn overlapping(&self, x: u8, y: u8, w: u8, h: u8) -> Vec<usize> {
        self.items.iter().enumerate().filter(|(_, p)| p.overlaps(x, y, w, h)).map(|(i, _)| i).collect()
    }

    pub fn fits(&self, size: (u8, u8), x: u8, y: u8) -> bool {
        self.in_bounds(x, y, size.0, size.1) && self.overlapping(x, y, size.0, size.1).is_empty()
    }

    /// Первое свободное место (слева направо, сверху вниз).
    pub fn find_free(&self, size: (u8, u8)) -> Option<(u8, u8)> {
        for y in 0..self.h {
            for x in 0..self.w {
                if self.fits(size, x, y) {
                    return Some((x, y));
                }
            }
        }
        None
    }
}

/// Где предмет лежит внутри инвентаря.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loc {
    Grid { x: u8, y: u8 },
    Equip(EquipSlot),
}

/// Куда положить предмет.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Grid { x: u8, y: u8 },
    Equip(EquipSlot),
    /// Сначала в подходящие стопки, потом в первое свободное место сетки.
    Auto,
    /// Надеть, если слот свободен; иначе как Auto.
    EquipOrAuto,
}

impl From<Loc> for Target {
    fn from(l: Loc) -> Self {
        match l {
            Loc::Grid { x, y } => Target::Grid { x, y },
            Loc::Equip(s) => Target::Equip(s),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveError {
    NotFound,
    NoSpace,
    WrongSlot,
}

impl MoveError {
    pub fn message(self) -> &'static str {
        match self {
            MoveError::NotFound => "Предмет уже забрали",
            MoveError::NoSpace => "Нет места",
            MoveError::WrongSlot => "Сюда это не надеть",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Inventory {
    pub grid: Grid,
    /// Надетые вещи по порядку [`EquipSlot::ALL`].
    pub equipment: Vec<Option<Item>>,
}

impl Inventory {
    pub fn new(size: (u8, u8)) -> Self {
        Self { grid: Grid::new(size.0, size.1), equipment: vec![None; EquipSlot::ALL.len()] }
    }

    pub fn equipped(&self, slot: EquipSlot) -> Option<&Item> {
        self.equipment[slot.index()].as_ref()
    }

    pub fn is_empty(&self) -> bool {
        self.grid.items.is_empty() && self.equipment.iter().all(Option::is_none)
    }

    pub fn find(&self, uid: ItemUid) -> Option<(Loc, &Item)> {
        if let Some(p) = self.grid.items.iter().find(|p| p.item.uid == uid) {
            return Some((Loc::Grid { x: p.x, y: p.y }, &p.item));
        }
        EquipSlot::ALL
            .iter()
            .find_map(|s| self.equipment[s.index()].as_ref().filter(|i| i.uid == uid).map(|i| (Loc::Equip(*s), i)))
    }

    /// Все предметы (сетка и экипировка).
    pub fn all_items(&self) -> impl Iterator<Item = &Item> {
        self.grid.items.iter().map(|p| &p.item).chain(self.equipment.iter().flatten())
    }

    /// Вынимает предмет и сообщает, где он лежал.
    pub fn take(&mut self, uid: ItemUid) -> Option<(Item, Loc)> {
        if let Some(i) = self.grid.items.iter().position(|p| p.item.uid == uid) {
            let p = self.grid.items.swap_remove(i);
            return Some((p.item, Loc::Grid { x: p.x, y: p.y }));
        }
        for s in EquipSlot::ALL {
            if self.equipment[s.index()].as_ref().is_some_and(|i| i.uid == uid) {
                return self.equipment[s.index()].take().map(|i| (i, Loc::Equip(s)));
            }
        }
        None
    }

    /// Кладёт предмет. `Ok(None)` — положили целиком; `Ok(Some(x))` — положили, но
    /// вытеснили предмет `x` (обмен местами) или остался излишек стопки;
    /// `Err(item)` — не получилось, предмет возвращается.
    pub fn place(&mut self, mut item: Item, target: Target) -> Result<Option<Item>, Item> {
        match target {
            Target::Equip(slot) => {
                if item.info().slot != Some(slot) {
                    return Err(item);
                }
                Ok(self.equipment[slot.index()].replace(item))
            }
            Target::EquipOrAuto => match item.info().slot {
                Some(slot) if self.equipment[slot.index()].is_none() => self.place(item, Target::Equip(slot)),
                _ => self.place(item, Target::Auto),
            },
            Target::Auto => {
                // Сначала доливаем в стопки.
                for p in self.grid.items.iter_mut() {
                    if p.item.can_stack_with(&item) {
                        let room = p.item.info().max_stack.saturating_sub(p.item.count);
                        let moved = room.min(item.count);
                        p.item.count += moved;
                        item.count -= moved;
                        if item.count == 0 {
                            return Ok(None);
                        }
                    }
                }
                match self.grid.find_free(item.size()) {
                    Some((x, y)) => {
                        self.grid.items.push(Placed { item, x, y });
                        Ok(None)
                    }
                    None => Err(item),
                }
            }
            Target::Grid { x, y } => {
                let (w, h) = item.size();
                if !self.grid.in_bounds(x, y, w, h) {
                    return Err(item);
                }
                let hits = self.grid.overlapping(x, y, w, h);
                match hits.as_slice() {
                    [] => {
                        self.grid.items.push(Placed { item, x, y });
                        Ok(None)
                    }
                    [i] if self.grid.items[*i].item.can_stack_with(&item) => {
                        let p = &mut self.grid.items[*i];
                        let room = p.item.info().max_stack.saturating_sub(p.item.count);
                        let moved = room.min(item.count);
                        p.item.count += moved;
                        item.count -= moved;
                        Ok(if item.count == 0 { None } else { Some(item) })
                    }
                    [i] => {
                        // Обмен: вынимаем мешающий предмет, если наш тогда влезает.
                        let other = self.grid.items.swap_remove(*i);
                        if self.grid.fits((w, h), x, y) {
                            self.grid.items.push(Placed { item, x, y });
                            Ok(Some(other.item))
                        } else {
                            self.grid.items.push(other);
                            Err(item)
                        }
                    }
                    _ => Err(item),
                }
            }
        }
    }

    /// Кладёт ровно в это место, только если оно свободно (без обменов и стопок).
    fn place_if_free(&mut self, item: Item, loc: Loc) -> Result<(), Item> {
        let free = match loc {
            Loc::Grid { x, y } => self.grid.fits(item.size(), x, y),
            Loc::Equip(s) => item.info().slot == Some(s) && self.equipment[s.index()].is_none(),
        };
        if free {
            self.restore(item, loc);
            Ok(())
        } else {
            Err(item)
        }
    }

    /// Возвращает предмет туда, откуда его вынули (место гарантированно свободно).
    fn restore(&mut self, item: Item, loc: Loc) {
        match loc {
            Loc::Grid { x, y } => self.grid.items.push(Placed { item, x, y }),
            Loc::Equip(s) => self.equipment[s.index()] = Some(item),
        }
    }

    /// Сумма характеристик надетых вещей.
    pub fn equipped_stats(&self) -> StatTotals {
        let mut totals = StatTotals::default();
        for item in self.equipment.iter().flatten() {
            for (s, v) in item.stats().0 {
                totals.add(s, v);
            }
        }
        totals
    }

    /// Положить список вещей: сначала надеть (если слот пуст), остальное в сетку.
    /// Возвращает то, что не влезло.
    pub fn fill(&mut self, items: Vec<Item>, equip: bool) -> Vec<Item> {
        let mut rest = Vec::new();
        for item in items {
            let target = if equip { Target::EquipOrAuto } else { Target::Auto };
            if let Err(item) = self.place(item, target) {
                rest.push(item);
            }
        }
        rest
    }
}

/// Перемещение предмета между инвентарями из среза (`src` и `dst` могут совпадать).
/// При любой неудаче всё возвращается как было.
pub fn transfer(invs: &mut [Inventory], src: usize, dst: usize, uid: ItemUid, target: Target) -> Result<(), MoveError> {
    // Инвентари маленькие, поэтому проще всего откатываться к копии.
    let backup = (invs[src].clone(), invs[dst].clone());
    let rollback = |invs: &mut [Inventory], err| {
        invs[src] = backup.0.clone();
        invs[dst] = backup.1.clone();
        Err(err)
    };
    let (item, from) = invs[src].take(uid).ok_or(MoveError::NotFound)?;
    if let Target::Equip(slot) = target
        && item.info().slot != Some(slot)
    {
        return rollback(invs, MoveError::WrongSlot);
    }
    let displaced = match invs[dst].place(item, target) {
        Ok(None) => return Ok(()),
        Ok(Some(d)) => d,
        Err(_) => return rollback(invs, MoveError::NoSpace),
    };
    // Вытесненный предмет (или излишек стопки) идёт на освободившееся место,
    // иначе в любое свободное место сначала там, откуда брали, потом туда, куда клали.
    let Err(displaced) = invs[src].place_if_free(displaced, from) else { return Ok(()) };
    let Err(displaced) = invs[src].place(displaced, Target::Auto) else { return Ok(()) };
    match invs[dst].place(displaced, Target::Auto) {
        Ok(_) => Ok(()),
        Err(_) => rollback(invs, MoveError::NoSpace),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::*;

    fn make(name: &str, uids: &mut UidGen) -> Item {
        roll_item(ItemBaseId::by_name(name), Quality::Common, 1, &mut Rng::new(3), uids)
    }

    #[test]
    fn place_and_overlap() {
        let mut uids = UidGen::default();
        let mut inv = Inventory::new((4, 4));
        let sword = make("Спата", &mut uids); // 1x3
        let id = sword.uid;
        assert_eq!(inv.place(sword, Target::Grid { x: 0, y: 0 }), Ok(None));
        assert!(!inv.grid.fits((1, 1), 0, 2));
        assert!(inv.grid.fits((1, 1), 0, 3));
        assert_eq!(inv.find(id).unwrap().0, Loc::Grid { x: 0, y: 0 });
        let big = make("Ламеллярный панцирь", &mut uids); // 2x3
        assert!(inv.place(big, Target::Grid { x: 3, y: 0 }).is_err()); // не влезает по ширине
    }

    #[test]
    fn equip_and_swap() {
        let mut uids = UidGen::default();
        let mut invs = vec![Inventory::new((6, 6))];
        let a = make("Спата", &mut uids);
        let b = make("Кавалерийская спата", &mut uids);
        let (ida, idb) = (a.uid, b.uid);
        invs[0].place(a, Target::Auto).unwrap();
        invs[0].place(b, Target::Auto).unwrap();
        transfer(&mut invs, 0, 0, ida, Target::Equip(EquipSlot::MainHand)).unwrap();
        assert_eq!(invs[0].equipped(EquipSlot::MainHand).unwrap().uid, ida);
        transfer(&mut invs, 0, 0, idb, Target::Equip(EquipSlot::MainHand)).unwrap();
        assert_eq!(invs[0].equipped(EquipSlot::MainHand).unwrap().uid, idb);
        assert!(matches!(invs[0].find(ida), Some((Loc::Grid { .. }, _))));
        assert_eq!(transfer(&mut invs, 0, 0, ida, Target::Equip(EquipSlot::Head)), Err(MoveError::WrongSlot));
    }

    #[test]
    fn loot_from_corpse_and_stack() {
        let mut uids = UidGen::default();
        let mut rng = Rng::new(9);
        let gold = ItemBaseId::by_name("Фоллис");
        let mut invs = vec![Inventory::new((3, 3)), Inventory::new((3, 3))];
        invs[0].place(roll_item(gold, Quality::Common, 90, &mut rng, &mut uids), Target::Auto).unwrap();
        let g2 = roll_item(gold, Quality::Common, 30, &mut rng, &mut uids);
        let g2id = g2.uid;
        invs[1].place(g2, Target::Auto).unwrap();
        transfer(&mut invs, 1, 0, g2id, Target::Auto).unwrap();
        let counts: Vec<u16> = invs[0].all_items().map(|i| i.count).collect();
        assert_eq!(counts.iter().sum::<u16>(), 120);
        assert!(invs[1].is_empty());
    }

    #[test]
    fn full_inventory_rolls_back() {
        let mut uids = UidGen::default();
        let mut invs = vec![Inventory::new((1, 3)), Inventory::new((4, 4))];
        invs[0].place(make("Спата", &mut uids), Target::Auto).unwrap();
        let axe = make("Боевой топор", &mut uids);
        let id = axe.uid;
        invs[1].place(axe, Target::Auto).unwrap();
        assert_eq!(transfer(&mut invs, 1, 0, id, Target::Auto), Err(MoveError::NoSpace));
        assert!(invs[1].find(id).is_some());
    }
}
