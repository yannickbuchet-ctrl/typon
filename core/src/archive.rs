//! Archives : ZIP (stocké ou « deflate »), TAR, TAR compressé par gzip (.tgz), et compression Unix « .Z » (LZW) des
//! anciens fichiers ODB++. Lecteurs minimaux, sans dépendance autre que miniz_oxide pour « deflate ».

use miniz_oxide::inflate::decompress_to_vec;

pub struct Fichier {
    pub chemin: String,
    pub octets: Vec<u8>,
}

/// Taille maximale décompressée acceptée pour une archive (protection contre les « bombes » de compression).
const MAX_TOTAL: usize = 1 << 30;

pub fn est_zip(o: &[u8]) -> bool {
    o.starts_with(b"PK\x03\x04") || o.starts_with(b"PK\x05\x06")
}
pub fn est_gzip(o: &[u8]) -> bool {
    o.starts_with(&[0x1f, 0x8b])
}
pub fn est_lzw(o: &[u8]) -> bool {
    o.starts_with(&[0x1f, 0x9d])
}
pub fn est_tar(o: &[u8]) -> bool {
    o.len() >= 512 && &o[257..262] == b"ustar"
}

/// Développe une archive (zip, tar, tgz, gz, Z), y compris les archives contenues dans d'autres. Un fichier simple
/// est renvoyé tel quel.
pub fn developper(nom: &str, octets: Vec<u8>) -> Result<Vec<Fichier>, String> {
    let mut sortie = Vec::new();
    let mut total = 0usize;
    developper_dans(nom, octets, &mut sortie, &mut total, 0)?;
    Ok(sortie)
}

fn developper_dans(nom: &str, octets: Vec<u8>, sortie: &mut Vec<Fichier>, total: &mut usize, profondeur: u32) -> Result<(), String> {
    if profondeur > 4 {
        return Ok(());
    }
    *total += octets.len();
    if *total > MAX_TOTAL {
        return Err("Archive trop volumineuse une fois décompressée (plus de 1 Go).".into());
    }
    let prefixe = |c: &str| -> String {
        if profondeur == 0 { c.to_string() } else { format!("{}/{}", sans_extension_archive(nom), c) }
    };
    if est_zip(&octets) {
        for f in lire_zip(&octets)? {
            let chemin = prefixe(&f.chemin);
            developper_dans(&chemin, f.octets, sortie, total, profondeur + 1)?;
        }
    } else if est_gzip(&octets) {
        let d = gunzip(&octets)?;
        let interne = sans_extension_archive(nom);
        let interne = if interne == nom { format!("{nom}.contenu") } else { interne };
        developper_dans(&interne, d, sortie, total, profondeur)?;
    } else if est_tar(&octets) {
        for f in lire_tar(&octets)? {
            let chemin = prefixe(&f.chemin);
            developper_dans(&chemin, f.octets, sortie, total, profondeur + 1)?;
        }
    } else if est_lzw(&octets) {
        let d = lzw(&octets)?;
        let interne = nom.strip_suffix(".Z").or(nom.strip_suffix(".z")).unwrap_or(nom).to_string();
        developper_dans(&interne, d, sortie, total, profondeur)?;
    } else {
        sortie.push(Fichier { chemin: nom.to_string(), octets });
    }
    Ok(())
}

fn sans_extension_archive(nom: &str) -> String {
    let bas = nom.to_ascii_lowercase();
    for ext in [".tar.gz", ".tgz", ".zip", ".tar", ".gz", ".z"] {
        if bas.ends_with(ext) {
            return nom[..nom.len() - ext.len()].to_string();
        }
    }
    nom.to_string()
}

fn u16le(o: &[u8], i: usize) -> usize {
    o.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).unwrap_or(0)
}
fn u32le(o: &[u8], i: usize) -> usize {
    o.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).unwrap_or(0)
}

pub fn lire_zip(o: &[u8]) -> Result<Vec<Fichier>, String> {
    // Fin du répertoire central : signature PK\x05\x06, cherchée depuis la fin.
    let min = o.len().saturating_sub(65_557);
    let fin = (min..o.len().saturating_sub(21)).rev().find(|&i| &o[i..i + 4] == b"PK\x05\x06").ok_or("Archive ZIP abîmée (répertoire introuvable).")?;
    let nombre = u16le(o, fin + 10);
    let mut p = u32le(o, fin + 16);
    let mut fichiers = Vec::new();
    for _ in 0..nombre {
        if o.get(p..p + 4) != Some(b"PK\x01\x02") {
            return Err("Archive ZIP abîmée (entrée illisible).".into());
        }
        let methode = u16le(o, p + 10);
        let taille_c = u32le(o, p + 20);
        let lnom = u16le(o, p + 28);
        let lextra = u16le(o, p + 30);
        let lcomm = u16le(o, p + 32);
        let local = u32le(o, p + 42);
        let drapeaux = u16le(o, p + 8);
        let nom_octets = o.get(p + 46..p + 46 + lnom).unwrap_or(&[]);
        let nom = if drapeaux & 0x800 != 0 { String::from_utf8_lossy(nom_octets).into_owned() } else { nom_octets.iter().map(|&c| cp437(c)).collect() };
        p += 46 + lnom + lextra + lcomm;
        if nom.ends_with('/') {
            continue;
        }
        if drapeaux & 1 != 0 {
            return Err(format!("« {nom} » est chiffré dans l'archive : ouvrez-la et décompressez-la d'abord."));
        }
        let ln = u16le(o, local + 26);
        let le = u16le(o, local + 28);
        let debut = local + 30 + ln + le;
        let donnees = o.get(debut..debut + taille_c).ok_or("Archive ZIP tronquée.")?;
        let octets = match methode {
            0 => donnees.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec(donnees).map_err(|_| format!("« {nom} » : décompression impossible."))?,
            m => return Err(format!("« {nom} » : méthode de compression {m} non prise en charge.")),
        };
        fichiers.push(Fichier { chemin: nom, octets });
    }
    Ok(fichiers)
}

fn cp437(c: u8) -> char {
    if c < 128 { c as char } else { '_' }
}

pub fn gunzip(o: &[u8]) -> Result<Vec<u8>, String> {
    if o.len() < 18 {
        return Err("Fichier gzip tronqué.".into());
    }
    let drapeaux = o[3];
    let mut p = 10;
    if drapeaux & 4 != 0 {
        p += 2 + u16le(o, p);
    }
    if drapeaux & 8 != 0 {
        while p < o.len() && o[p] != 0 {
            p += 1;
        }
        p += 1;
    }
    if drapeaux & 16 != 0 {
        while p < o.len() && o[p] != 0 {
            p += 1;
        }
        p += 1;
    }
    if drapeaux & 2 != 0 {
        p += 2;
    }
    let corps = o.get(p..).ok_or("Fichier gzip tronqué.")?;
    let _ = decompress_to_vec;
    miniz_oxide::inflate::decompress_to_vec_with_limit(corps, MAX_TOTAL).map_err(|_| "Décompression gzip impossible (fichier abîmé ?).".into())
}

pub fn lire_tar(o: &[u8]) -> Result<Vec<Fichier>, String> {
    let mut fichiers = Vec::new();
    let mut p = 0;
    let mut nom_long: Option<String> = None;
    while p + 512 <= o.len() {
        let entete = &o[p..p + 512];
        if entete.iter().all(|&b| b == 0) {
            break;
        }
        let champ = |d: usize, l: usize| -> String {
            let s = &entete[d..d + l];
            let f = s.iter().position(|&b| b == 0).unwrap_or(l);
            String::from_utf8_lossy(&s[..f]).into_owned()
        };
        let taille = usize::from_str_radix(champ(124, 12).trim(), 8).unwrap_or(0);
        let genre = entete[156];
        let mut nom = champ(0, 100);
        let prefixe = champ(345, 155);
        if !prefixe.is_empty() && &entete[257..262] == b"ustar" {
            nom = format!("{prefixe}/{nom}");
        }
        let debut = p + 512;
        let donnees = o.get(debut..debut + taille).ok_or("Archive TAR tronquée.")?;
        p = debut + taille.div_ceil(512) * 512;
        match genre {
            b'L' => {
                nom_long = Some(String::from_utf8_lossy(donnees).trim_end_matches('\0').to_string());
                continue;
            }
            b'x' | b'g' => {
                // En-tête étendu PAX : « path=… » donne le nom complet.
                let t = String::from_utf8_lossy(donnees);
                for l in t.lines() {
                    if let Some(i) = l.find(" path=") {
                        nom_long = Some(l[i + 6..].to_string());
                    }
                }
                continue;
            }
            b'0' | 0 | b'7' => {}
            _ => {
                nom_long = None;
                continue;
            }
        }
        let chemin = nom_long.take().unwrap_or(nom);
        fichiers.push(Fichier { chemin, octets: donnees.to_vec() });
    }
    Ok(fichiers)
}

/// Décompression LZW (« compress » Unix, fichiers .Z).
pub fn lzw(o: &[u8]) -> Result<Vec<u8>, String> {
    if o.len() < 3 {
        return Err("Fichier .Z tronqué.".into());
    }
    let max_bits = (o[2] & 0x1f) as u32;
    let bloc = o[2] & 0x80 != 0;
    let mut prefixes: Vec<u32> = Vec::with_capacity(1 << max_bits.min(16));
    let mut suffixes: Vec<u8> = Vec::with_capacity(1 << max_bits.min(16));
    for i in 0..256u32 {
        prefixes.push(u32::MAX);
        suffixes.push(i as u8);
    }
    if bloc {
        prefixes.push(u32::MAX);
        suffixes.push(0);
    }
    let mut sortie = Vec::new();
    let mut bits = 9u32;
    let mut pos_bit: usize = 3 * 8;
    let total_bits = o.len() * 8;
    let mut ancien: Option<u32> = None;
    let mut pile: Vec<u8> = Vec::new();
    let mut lus_groupe = 0usize; // codes lus depuis le dernier changement de taille
    while pos_bit + bits as usize <= total_bits {
        let mut code = 0u32;
        for k in 0..bits {
            let b = pos_bit + k as usize;
            if o[b / 8] >> (b % 8) & 1 != 0 {
                code |= 1 << k;
            }
        }
        pos_bit += bits as usize;
        lus_groupe += 1;
        if bloc && code == 256 {
            // Remise à zéro du dictionnaire ; les codes sont lus par groupes de 8 : on saute à la fin du groupe.
            let reste = (8 - lus_groupe % 8) % 8;
            pos_bit += reste * bits as usize;
            prefixes.truncate(257);
            suffixes.truncate(257);
            bits = 9;
            ancien = None;
            lus_groupe = 0;
            continue;
        }
        let premier;
        if (code as usize) < prefixes.len() {
            let mut c = code;
            pile.clear();
            while c != u32::MAX {
                pile.push(suffixes[c as usize]);
                c = prefixes[c as usize];
            }
            premier = *pile.last().unwrap();
            if let Some(a) = ancien {
                prefixes.push(a);
                suffixes.push(premier);
            }
        } else if let Some(a) = ancien {
            // Cas KwKwK : le code n'existe pas encore.
            let mut c = a;
            pile.clear();
            while c != u32::MAX {
                pile.push(suffixes[c as usize]);
                c = prefixes[c as usize];
            }
            premier = *pile.last().unwrap();
            pile.insert(0, premier);
            prefixes.push(a);
            suffixes.push(premier);
        } else {
            return Err("Fichier .Z abîmé.".into());
        }
        sortie.extend(pile.iter().rev());
        ancien = Some(code);
        if prefixes.len() >= (1 << bits) && bits < max_bits {
            let reste = (8 - lus_groupe % 8) % 8;
            pos_bit += reste * bits as usize;
            bits += 1;
            lus_groupe = 0;
        }
        if sortie.len() > MAX_TOTAL {
            return Err("Fichier .Z trop volumineux.".into());
        }
    }
    Ok(sortie)
}
