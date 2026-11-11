//! Fichiers abîmés ou piégés (cas trouvés par la revue de code) : le moteur ne doit ni paniquer (en version publiée,
//! une panique arrête le module WebAssembly), ni s'emballer en mémoire ou en temps.
//!
//!   cargo test --release --test fichiers_hostiles -- --nocapture

use std::panic::catch_unwind;
use std::time::Instant;
use typon_core::archive::Fichier;
use typon_core::{Lot, analyser};

fn lire(fichiers: Vec<(&'static str, Vec<u8>)>) -> serde_json::Value {
    let mut l = Lot::default();
    for (n, o) in fichiers {
        l.fichiers.push(Fichier { chemin: n.into(), octets: o });
    }
    analyser(&mut l)
}

/// Lit sans paniquer, en moins de `secondes`, et renvoie la description.
fn sans_panique(nom: &str, fichiers: Vec<(&'static str, Vec<u8>)>, secondes: f64) -> serde_json::Value {
    let t0 = Instant::now();
    let r = catch_unwind(move || lire(fichiers));
    let duree = t0.elapsed().as_secs_f64();
    let v = r.unwrap_or_else(|_| panic!("{nom} : panique"));
    println!("{nom} : {duree:.2} s, alertes {}", alertes(&v));
    assert!(duree < secondes, "{nom} : {duree:.1} s");
    v
}

/// Toutes les alertes : générales et par couche.
fn alertes(v: &serde_json::Value) -> String {
    let mut a = v["alertes"].to_string();
    for c in v["couches"].as_array().map(|x| x.as_slice()).unwrap_or(&[]) {
        a += &c["meta"]["alertes"].to_string();
    }
    a
}

#[test]
fn dates_et_nombres_invalides() {
    let mut g = b"%TF.CreationDate,2019-".to_vec();
    g.push(0xFF);
    g.extend_from_slice(b"6-03T14:22:52*%\n%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.1*%\nD10*\nX0Y0D03*\nM02*\n");
    sans_panique("date X2 non UTF-8", vec![("a.gtl", g)], 5.0);
    sans_panique("ouverture NaN puis trait", vec![("a.gtl", b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10R,NaNX1*%\nD10*\nX0Y0D02*\nX1000000Y1000000D01*\nM02*\n".to_vec())], 5.0);
    sans_panique("ouverture infinie", vec![("a.gtl", b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,1e999*%\nD10*\nX0Y0D03*\nX1Y1D03*\nM02*\n".to_vec())], 5.0);
    let mut info = b"CREATION_DATE=2016".to_vec();
    info.push(0xFF);
    info.extend_from_slice(b"024.101454\n");
    sans_panique("date ODB++ non UTF-8", vec![("j/matrix/matrix", b"STEP {\nNAME=pcb\n}\n".to_vec()), ("j/misc/info", info)], 5.0);
    sans_panique("ODB++ : index de symbole démesuré", vec![("j/matrix/matrix", b"STEP {\nCOL=1\nNAME=pcb\n}\nLAYER {\nROW=1\nCONTEXT=BOARD\nTYPE=SIGNAL\nNAME=top\n}\n".to_vec()), ("j/steps/pcb/layers/top/features", b"$18446744073709551615 r10\n$4000000000 r10\nP 0 0 0 P 0 0\n".to_vec())], 5.0);
    sans_panique("date du fichier de job multi-octets", vec![("a.gbrjob", "{\"Header\":{\"CreationDate\":\"2019-€€€€€€€€€€\"}}".as_bytes().to_vec())], 5.0);
}

#[test]
fn amplifications() {
    // Répétition démesurée : refusée (au plus 100 000 copies).
    let v = sans_panique("SR 100000 × 100000", vec![("a.gtl", b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.1*%\n%SRX100000Y100000I0.1J0.1*%\nD10*\nX0Y0D03*\n%SR*%\nM02*\n".to_vec())], 10.0);
    assert!(alertes(&v).contains("Répétition"), "alerte de répétition attendue");
    // Répétitions qui dépassent le plafond d'éléments : tronquées avec une alerte.
    let mut s = String::from("%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.01*%\n%SRX300Y300I0.1J0.1*%\nD10*\n");
    for i in 0..100 {
        s += &format!("X{}Y0D03*\n", i * 1000);
    }
    s += "%SR*%\nM02*\n";
    let v = sans_panique("SR 300 × 300 de 100 flashs (9 M)", vec![("a.gtl", s.into_bytes())], 60.0);
    assert!(alertes(&v).contains("éléments"), "alerte de plafond attendue : {}", alertes(&v));
    // Polygone à 4 milliards de sommets, moiré sans fin, expression démesurée.
    sans_panique("polygone de 4e9 sommets", vec![("a.gtl", b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10P,1X4000000000*%\nD10*\nX0Y0D03*\nM02*\n".to_vec())], 5.0);
    sans_panique("moiré sans fin", vec![("a.gtl", b"%FSLAX46Y46*%\n%MOMM*%\n%AMM*6,0,0,1,0,0,1000000000,0.1,1,0*%\n%ADD10M*%\nD10*\nX0Y0D03*\nM02*\n".to_vec())], 5.0);
    let mut m = String::from("%FSLAX46Y46*%\n%MOMM*%\n%AMP*1,1,");
    m += &"(".repeat(5000);
    m += "1";
    m += &")".repeat(5000);
    m += ",0,0*%\n%ADD10P*%\nD10*\nX0Y0D03*\nM02*\n";
    sans_panique("parenthèses imbriquées", vec![("a.gtl", m.into_bytes())], 5.0);
    let mut m = String::from("%FSLAX46Y46*%\n%MOMM*%\n%AMP*1,1,");
    m += &"-".repeat(100_000);
    m += "1,0,0*%\n%ADD10P*%\nD10*\nX0Y0D03*\nM02*\n";
    sans_panique("signes imbriqués", vec![("a.gtl", m.into_bytes())], 5.0);
}

#[test]
fn archives() {
    // Bombe ZIP : 1 Go de zéros compressés (environ 1 Mo), refusée proprement.
    let zeros = vec![0u8; 1 << 30];
    let comprime = miniz_oxide::deflate::compress_to_vec(&zeros, 6);
    drop(zeros);
    let mut zip = Vec::new();
    let nom = b"bombe.gbr";
    let crc = 0u32;
    let entete = |sig: u32, v: &mut Vec<u8>| v.extend_from_slice(&sig.to_le_bytes());
    entete(0x0403_4b50, &mut zip);
    for x in [20u16, 0, 8, 0, 0] {
        zip.extend_from_slice(&x.to_le_bytes());
    }
    zip.extend_from_slice(&crc.to_le_bytes());
    zip.extend_from_slice(&(comprime.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(1u32 << 30).to_le_bytes());
    zip.extend_from_slice(&(nom.len() as u16).to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    zip.extend_from_slice(nom);
    zip.extend_from_slice(&comprime);
    let central = zip.len();
    entete(0x0201_4b50, &mut zip);
    for x in [20u16, 20, 0, 8, 0, 0] {
        zip.extend_from_slice(&x.to_le_bytes());
    }
    zip.extend_from_slice(&crc.to_le_bytes());
    zip.extend_from_slice(&(comprime.len() as u32).to_le_bytes());
    zip.extend_from_slice(&(1u32 << 30).to_le_bytes());
    zip.extend_from_slice(&(nom.len() as u16).to_le_bytes());
    for x in [0u16, 0, 0, 0] {
        zip.extend_from_slice(&x.to_le_bytes());
    }
    zip.extend_from_slice(&0u32.to_le_bytes());
    zip.extend_from_slice(&0u32.to_le_bytes());
    zip.extend_from_slice(nom);
    let taille_central = zip.len() - central;
    entete(0x0605_4b50, &mut zip);
    for x in [0u16, 0, 1, 1] {
        zip.extend_from_slice(&x.to_le_bytes());
    }
    zip.extend_from_slice(&(taille_central as u32).to_le_bytes());
    zip.extend_from_slice(&(central as u32).to_le_bytes());
    zip.extend_from_slice(&0u16.to_le_bytes());
    let v = sans_panique("bombe ZIP de 1 Go", vec![("bombe.zip", zip)], 30.0);
    assert!(alertes(&v).contains("400 Mo"), "refus attendu : {}", alertes(&v));
    // Gzip qui se contient lui-même indéfiniment : borné par la profondeur.
    let mut g = b"%FSLAX46Y46*%\n%MOMM*%\nM02*\n".to_vec();
    for _ in 0..12 {
        let c = miniz_oxide::deflate::compress_to_vec(&g, 6);
        let mut e = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff];
        e.extend_from_slice(&c);
        e.extend_from_slice(&[0; 8]);
        g = e;
    }
    sans_panique("gzip imbriqué 12 fois", vec![("a.gz", g)], 5.0);
}
