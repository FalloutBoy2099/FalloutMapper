//! Édition des scripts d'une carte : scripts d'objets (attacher / détacher), scripts spatiaux, script de la carte.
//!
//! Conventions reprises de Gecko (`ScriptEditService`) et vérifiées sur les cartes d'origine :
//! * une créature utilise la liste « Créature » (4), tout autre objet la liste « Objet » (3) ;
//! * l'objet désigne son script par l'identifiant de l'entrée (`pid` = type << 24 | numéro), dans le mot 16 ;
//! * l'entrée porte le numéro du programme dans `scripts.lst` (à partir de 0) et l'identifiant de son propriétaire ;
//! * les variables locales d'un script d'objet sont allouées par le moteur au chargement (décalage -1, compte 0).

use super::model::*;

pub const SCRIPT_TYPE_SPATIAL: usize = 1;
pub const SCRIPT_TYPE_ITEM: usize = 3;
pub const SCRIPT_TYPE_CRITTER: usize = 4;
const NONE: u32 = 0xFFFF_FFFF;

fn blank_entry() -> ScriptEntry {
    ScriptEntry {
        pid: 0,
        next: 0,
        typed: Vec::new(),
        tail: [0; SCRIPT_TAIL_WORDS],
    }
}

impl ScriptList {
    /// Tous les scripts utilisés, dans l'ordre.
    fn used_entries(&self) -> Vec<ScriptEntry> {
        self.iter().cloned().collect()
    }

    /// Reconstruit les groupes de 16 à partir d'une liste d'entrées (le dernier groupe est complété de vides).
    fn rebuild(&mut self, entries: Vec<ScriptEntry>) {
        let count = entries.len() as u32;
        let mut groups = Vec::new();
        for chunk in entries.chunks(SCRIPT_GROUP_SLOTS) {
            let mut slots = chunk.to_vec();
            let used = slots.len() as u32;
            slots.resize(SCRIPT_GROUP_SLOTS, blank_entry());
            groups.push(ScriptGroup {
                entries: slots,
                used,
                reserved: 0,
            });
        }
        *self = ScriptList { count, groups };
    }

    pub fn push(&mut self, entry: ScriptEntry) {
        let mut all = self.used_entries();
        all.push(entry);
        self.rebuild(all);
    }

    /// Retire l'entrée d'identifiant `pid`. Renvoie `true` si elle existait.
    pub fn remove_pid(&mut self, pid: u32) -> bool {
        let mut all = self.used_entries();
        let before = all.len();
        all.retain(|e| e.pid != pid);
        let removed = all.len() != before;
        if removed {
            self.rebuild(all);
        }
        removed
    }

    pub fn find_mut(&mut self, pid: u32) -> Option<&mut ScriptEntry> {
        let used: Vec<u32> = self.groups.iter().map(|g| g.used).collect();
        self.groups
            .iter_mut()
            .zip(used)
            .flat_map(|(g, u)| g.entries.iter_mut().take(u as usize))
            .find(|e| e.pid == pid)
    }
}

/// Liste de scripts qui convient à un objet.
pub fn script_section_for(o: &MapObject) -> usize {
    if o.object_type() == Some(crate::formats::ObjectType::Critter) {
        SCRIPT_TYPE_CRITTER
    } else {
        SCRIPT_TYPE_ITEM
    }
}

impl Map {
    fn next_script_number(&self, section: usize) -> u32 {
        self.scripts[section]
            .iter()
            .map(|e| (e.pid & 0x00FF_FFFF) + 1)
            .max()
            .unwrap_or(0)
    }

    /// Identifiant libre pour un objet ou pour le propriétaire d'un script.
    fn next_owner_id(&self) -> i32 {
        let scripts = self
            .scripts
            .iter()
            .flat_map(ScriptList::iter)
            .map(|e| e.tail[3])
            .max()
            .unwrap_or(0);
        self.max_object_id().max(scripts) + 1
    }

    fn object_script_entry(&self, section: usize, program: u32, owner: i32) -> ScriptEntry {
        let mut tail = [0i32; SCRIPT_TAIL_WORDS];
        tail[1] = program as i32;
        tail[3] = owner;
        tail[4] = -1; // décalage des variables locales : alloué par le moteur
        tail[9] = -1; // « action en cours »
        ScriptEntry {
            pid: ((section as u32) << 24) | self.next_script_number(section),
            next: NONE,
            typed: Vec::new(),
            tail,
        }
    }

    /// Attache le programme `program` (ligne de `scripts.lst`, à partir de 0) à un objet, en remplaçant son script actuel.
    pub fn attach_object_script(&mut self, elevation: usize, index: usize, program: u32) -> bool {
        let Some(o) = self.objects.get(elevation).and_then(|v| v.get(index)) else {
            return false;
        };
        let section = script_section_for(o);
        let old = o.words[word::SCRIPT_ID];
        if old != -1 {
            self.remove_script(old as u32);
        }
        let owner = self.next_owner_id();
        let entry = self.object_script_entry(section, program, owner);
        let pid = entry.pid;
        self.scripts[section].push(entry);
        let o = &mut self.objects[elevation][index];
        o.words[word::ID] = owner;
        o.words[word::SCRIPT_ID] = pid as i32;
        if section == SCRIPT_TYPE_CRITTER {
            o.words[word::SCRIPT_INDEX] = program as i32;
        }
        true
    }

    /// Détache le script d'un objet et supprime son entrée.
    pub fn detach_object_script(&mut self, elevation: usize, index: usize) -> bool {
        let Some(o) = self.objects.get(elevation).and_then(|v| v.get(index)) else {
            return false;
        };
        let sid = o.words[word::SCRIPT_ID];
        if sid == -1 {
            return false;
        }
        self.remove_script(sid as u32);
        let o = &mut self.objects[elevation][index];
        o.words[word::SCRIPT_ID] = -1;
        o.words[word::SCRIPT_INDEX] = -1;
        true
    }

    /// Supprime une entrée de script (de la liste de son type) ; les objets qui la désignaient doivent être mis à jour
    /// par l'appelant.
    pub fn remove_script(&mut self, pid: u32) -> bool {
        let section = (pid >> 24) as usize;
        self.scripts
            .get_mut(section)
            .is_some_and(|l| l.remove_pid(pid))
    }

    /// Nouveau script spatial : se déclenche à `tile` (élévation `elevation`) dans un rayon de `radius` hexagones.
    pub fn add_spatial_script(
        &mut self,
        program: u32,
        tile: u32,
        elevation: u32,
        radius: i32,
    ) -> u32 {
        let owner = self.next_owner_id();
        let mut entry = self.object_script_entry(SCRIPT_TYPE_SPATIAL, program, owner);
        entry.typed = vec![(elevation << 29) | (tile & 0x03FF_FFFF), radius as u32];
        let pid = entry.pid;
        self.scripts[SCRIPT_TYPE_SPATIAL].push(entry);
        pid
    }

    /// Modifie un script spatial en place (l'identifiant et le propriétaire restent).
    pub fn edit_spatial_script(
        &mut self,
        pid: u32,
        program: u32,
        tile: u32,
        elevation: u32,
        radius: i32,
    ) -> bool {
        let Some(e) = self.scripts[SCRIPT_TYPE_SPATIAL].find_mut(pid) else {
            return false;
        };
        e.tail[1] = program as i32;
        e.typed = vec![(elevation << 29) | (tile & 0x03FF_FFFF), radius as u32];
        true
    }

    /// Définit (ou retire avec `None`) le script de la carte : `scripts.lst` + 1 dans l'en-tête, -1 sans script.
    pub fn set_map_script(&mut self, program: Option<u32>) {
        self.header.script_id = program.map_or(-1, |p| p as i32 + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(pid: u32) -> ScriptEntry {
        ScriptEntry {
            pid,
            next: NONE,
            typed: Vec::new(),
            tail: [0; SCRIPT_TAIL_WORDS],
        }
    }

    #[test]
    fn list_keeps_groups_of_sixteen_and_counts() {
        let mut l = ScriptList::default();
        for i in 0..20 {
            l.push(entry(0x0300_0000 + i));
        }
        assert_eq!(l.count, 20);
        assert_eq!(l.groups.len(), 2);
        assert!(
            l.groups
                .iter()
                .all(|g| g.entries.len() == SCRIPT_GROUP_SLOTS)
        );
        assert_eq!((l.groups[0].used, l.groups[1].used), (16, 4));
        assert!(l.remove_pid(0x0300_0003));
        assert!(!l.remove_pid(0x0300_0003));
        assert_eq!((l.count, l.groups[0].used, l.groups[1].used), (19, 16, 3));
        assert_eq!(l.iter().count(), 19);
        for i in 0..3 {
            l.remove_pid(0x0300_0010 + i);
        }
        assert_eq!(l.groups.len(), 1, "un groupe vide disparaît");
    }
}
