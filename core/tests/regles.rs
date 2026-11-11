//! Cas précis signalés par la revue de code : DFM (diagonales longues, gros trous, plusieurs endroits pour une même
//! paire d'îlots), codes D modaux, couches négatives.
//!
//!   cargo test --release --test regles -- --nocapture

use serde_json::Value;
use typon_core::archive::Fichier;
use typon_core::geom::Prim;
use typon_core::{Lot, analyser, dfm};

fn lot(f: Vec<(&str, Vec<u8>)>) -> Lot {
    let mut l = Lot::default();
    for (n, o) in f {
        l.fichiers.push(Fichier { chemin: n.into(), octets: o });
    }
    analyser(&mut l);
    l
}

fn dfm_de(f: Vec<(&str, Vec<u8>)>) -> Value {
    dfm::verifier(&lot(f).couches, &dfm::Regles::depuis(&Value::Null))
}

fn compte(v: &Value, regle: &str) -> usize {
    v["resultats"].as_array().unwrap().iter().filter(|r| r["regle"] == regle).count()
}

/// Cuivre du dessus fait de traits de 0,2 mm.
fn traits(t: &[(f64, f64, f64, f64)]) -> Vec<u8> {
    let mut s = String::from("%TF.FileFunction,Copper,L1,Top*%\n%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.2*%\nD10*\n");
    for (x0, y0, x1, y1) in t {
        s += &format!("X{}Y{}D02*\nX{}Y{}D01*\n", (x0 * 1e6).round() as i64, (y0 * 1e6).round() as i64, (x1 * 1e6).round() as i64, (y1 * 1e6).round() as i64);
    }
    s += "M02*\n";
    s.into_bytes()
}

#[test]
fn dfm_diagonale_longue() {
    // Piste de 30 mm à 45° et piste parallèle à 0,05 mm, au milieu.
    let d = 0.25 / 2f64.sqrt();
    let v = dfm_de(vec![("a.gtl", traits(&[(0.0, 0.0, 30.0, 30.0), (10.0 + d, 10.0 - d, 20.0 + d, 20.0 - d)]))]);
    assert!(compte(&v, "isolement") >= 1, "diagonale longue : isolement non vu ({})", v["comptes"]);
    // Deux pistes parallèles trop proches sur 40 mm : plusieurs endroits signalés, pas un seul.
    let v = dfm_de(vec![("a.gtl", traits(&[(0.0, 0.0, 40.0, 0.0), (0.0, 0.25, 40.0, 0.25)]))]);
    assert!(compte(&v, "isolement") >= 10, "pistes parallèles : {} endroit(s) seulement", compte(&v, "isolement"));
}

#[test]
fn dfm_gros_trous() {
    // Trous de 6,0 et 3,2 mm à 0,1 mm bord à bord.
    let drl = b"M48\nMETRIC\nT1C6.000\nT2C3.200\n%\nT1\nX0.0Y0.0\nT2\nX4.7Y0.0\nM30\n".to_vec();
    let v = dfm_de(vec![("a.drl", drl)]);
    assert_eq!(compte(&v, "trouTrou"), 1, "gros trous : {}", v["comptes"]);
}

#[test]
fn codes_d_modaux() {
    // Flash, puis coordonnées sans code D : second flash, pas un trait.
    let l = lot(vec![("a.gtl", b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.5*%\nD10*\nX0Y0D03*\nX5000000Y0*\nM02*\n".to_vec())]);
    let e = &l.couches[0].dessin.elements;
    assert_eq!(e.len(), 2, "deux flashs attendus : {e:?}");
    assert!(e.iter().all(|x| matches!(x.prim, Prim::Cap { a, b, .. } if a.dist(b) < 1e-9)), "flashs attendus : {e:?}");
    // Trait, puis coordonnées sans code D : le trait continue.
    let l = lot(vec![("a.gtl", b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.5*%\nD10*\nX0Y0D02*\nX5000000Y0D01*\nX5000000Y5000000*\nM02*\n".to_vec())]);
    assert_eq!(l.couches[0].dessin.elements.len(), 2, "deux traits attendus");
}

#[test]
fn couche_negative() {
    // Cuivre en image négative (IP NEG) : un disque décrit un trou dans le cuivre.
    let l = lot(vec![("a.gtl", b"%TF.FileFunction,Copper,L1,Top*%\n%FSLAX46Y46*%\n%MOMM*%\n%IPNEG*%\n%ADD10C,1*%\nD10*\nX0Y0D03*\nX10000000Y10000000D03*\nM02*\n".to_vec())]);
    let d = &l.couches[0].dessin;
    assert!(d.elements[0].sombre && matches!(d.elements[0].prim, Prim::Poly { .. }), "fond attendu en premier");
    assert!(d.elements[1..].iter().all(|e| !e.sombre), "les disques deviennent des trous");
    // Vernis de KiCad déclaré « négatif » (ouvertures) : pas d'inversion.
    let l = lot(vec![("a.gts", b"%TF.FileFunction,Soldermask,Top*%\n%TF.FilePolarity,Negative*%\n%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,1*%\nD10*\nX0Y0D03*\nM02*\n".to_vec())]);
    assert!(l.couches[0].dessin.elements.iter().all(|e| e.sombre), "le vernis ne doit pas être inversé");
}
