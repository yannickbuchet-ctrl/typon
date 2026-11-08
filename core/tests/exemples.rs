//! Essais du moteur sur les fichiers d'exemple externes (téléchargés par scripts/telecharger-exemples.mjs).
//!
//!   cargo test --release -- --nocapture
//!
//! 1. Fichiers de test de la norme Gerber (Ucamco) : rendu comparé pixel par pixel à leur image de référence.
//! 2. Tous les jeux d'exemple : lecture complète, résumé (couches, rôles, objets, alertes) et durée.
//! Les rendus sont écrits dans ../essais/ pour être regardés.

use std::path::{Path, PathBuf};
use std::time::Instant;
use typon_core::archive::Fichier;
use typon_core::geom::{Dessin, Prim, pt};
use typon_core::{Lot, analyser, gerber};

fn racine() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn exemples() -> PathBuf {
    racine().join("exemples-externes")
}

/// Rendu simple (pixel par pixel) d'un dessin, dans l'ordre et avec les polarités. `cadre` : (x0, y0, x1, y1) en mm
/// placé sur l'image entière.
fn rendre(d: &Dessin, largeur: usize, hauteur: usize, cadre: [f64; 4]) -> Vec<bool> {
    let mut img = vec![false; largeur * hauteur];
    let sx = (cadre[2] - cadre[0]) / largeur as f64;
    let sy = (cadre[3] - cadre[1]) / hauteur as f64;
    for e in &d.elements {
        let b = e.prim.boite_englobante();
        let i0 = (((b.x0 - cadre[0]) / sx).floor().max(0.0) as usize).min(largeur);
        let i1 = (((b.x1 - cadre[0]) / sx).ceil().max(0.0) as usize).min(largeur);
        let j0 = (((cadre[3] - b.y1) / sy).floor().max(0.0) as usize).min(hauteur);
        let j1 = (((cadre[3] - b.y0) / sy).ceil().max(0.0) as usize).min(hauteur);
        // Trait fin : au moins un demi-pixel de large, comme à l'écran.
        let tol = match e.prim { Prim::Cap { r, .. } if r < sx => sx * 0.5, _ => 0.0 };
        for j in j0..j1 {
            for i in i0..i1 {
                let p = pt(cadre[0] + (i as f64 + 0.5) * sx, cadre[3] - (j as f64 + 0.5) * sy);
                if e.prim.contient(p, tol) {
                    img[j * largeur + i] = e.sombre;
                }
            }
        }
    }
    img
}

fn ecrire_png(chemin: &Path, img: &[bool], largeur: usize, hauteur: usize) {
    let f = std::fs::File::create(chemin).unwrap();
    let mut e = png::Encoder::new(std::io::BufWriter::new(f), largeur as u32, hauteur as u32);
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    let mut w = e.write_header().unwrap();
    let data: Vec<u8> = img.iter().flat_map(|&v| if v { [255, 94, 37] } else { [0, 0, 0] }).collect();
    w.write_image_data(&data).unwrap();
}

/// Image de référence : vrai là où elle est orange.
fn lire_reference(chemin: &Path) -> (Vec<bool>, usize, usize) {
    let decodeur = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(chemin).unwrap()));
    let mut lecteur = decodeur.read_info().unwrap();
    let mut buf = vec![0; lecteur.output_buffer_size().unwrap()];
    let info = lecteur.next_frame(&mut buf).unwrap();
    let canaux = info.color_type.samples();
    let (l, h) = (info.width as usize, info.height as usize);
    let img = (0..l * h).map(|i| {
        let p = &buf[i * canaux..i * canaux + 3.min(canaux)];
        p[0] > 128 && (canaux < 3 || p[2] < 128)
    }).collect();
    (img, l, h)
}

fn boite_pixels(img: &[bool], l: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for j in 0..h {
        for i in 0..l {
            if img[j * l + i] {
                x0 = x0.min(i);
                y0 = y0.min(j);
                x1 = x1.max(i);
                y1 = y1.max(j);
            }
        }
    }
    (x0 <= x1).then_some((x0, y0, x1 + 1, y1 + 1))
}

#[test]
fn fichiers_de_test_ucamco() {
    let dossier = exemples().join("ucamco/gerber-layer-format-test-files");
    let Some(dossier) = std::fs::read_dir(&dossier).ok().and_then(|mut d| d.next()).and_then(|e| e.ok()).map(|e| e.path()) else {
        eprintln!("Fichiers de test Ucamco absents : lancez scripts/telecharger-exemples.mjs");
        return;
    };
    let sortie = racine().join("essais/ucamco");
    std::fs::create_dir_all(&sortie).unwrap();
    let mut echecs = Vec::new();
    for entree in std::fs::read_dir(&dossier).unwrap() {
        let chemin = entree.unwrap().path();
        if chemin.extension().map(|e| e != "gbr").unwrap_or(true) {
            continue;
        }
        let reference = chemin.with_extension("png");
        let texte = std::fs::read_to_string(&chemin).unwrap();
        let g = gerber::lire(&texte);
        let nom = chemin.file_name().unwrap().to_string_lossy().to_string();
        if !reference.exists() {
            println!("{nom:45} {} objets, sans image de référence", g.dessin.objets.len());
            continue;
        }
        let (refimg, l, h) = lire_reference(&reference);
        let Some((rx0, ry0, rx1, ry1)) = boite_pixels(&refimg, l, h) else { continue };
        // Le cadre en mm qui fait coïncider la boîte du dessin avec la boîte orange de la référence.
        let b = g.dessin.boite();
        let kx = (b.x1 - b.x0) / (rx1 - rx0) as f64;
        let ky = (b.y1 - b.y0) / (ry1 - ry0) as f64;
        let k = kx.max(ky);
        let cadre = [b.x0 - rx0 as f64 * k, b.y1 - h as f64 * k + ry0 as f64 * k, b.x0 + (l - rx0) as f64 * k, b.y1 + ry0 as f64 * k];
        let img = rendre(&g.dessin, l, h, cadre);
        ecrire_png(&sortie.join(format!("{nom}.png")), &img, l, h);
        // Un pixel n'est compté faux que si aucun pixel de même valeur n'existe à 2 pixels près dans l'autre image.
        let voisin = |im: &[bool], i: usize, j: usize, v: bool| -> bool {
            for dj in -2i64..=2 {
                for di in -2i64..=2 {
                    let (x, y) = (i as i64 + di, j as i64 + dj);
                    if x >= 0 && y >= 0 && (x as usize) < l && (y as usize) < h && im[y as usize * l + x as usize] == v {
                        return true;
                    }
                }
            }
            false
        };
        let mut differents = 0;
        for j in 0..h {
            for i in 0..l {
                let (a, r) = (img[j * l + i], refimg[j * l + i]);
                if a != r && !voisin(&refimg, i, j, a) || a != r && !voisin(&img, i, j, r) {
                    differents += 1;
                }
            }
        }
        let orange = refimg.iter().filter(|&&v| v).count().max(1);
        let ecart = differents as f64 / orange as f64 * 100.0;
        println!("{nom:45} {:6} objets, écart avec la référence : {ecart:5.2} % des pixels orange{}", g.dessin.objets.len(), if g.alertes.is_empty() { String::new() } else { format!(" — alertes : {:?}", g.alertes) });
        if ecart > 6.0 {
            echecs.push(format!("{nom} : {ecart:.1} %"));
        }
    }
    assert!(echecs.is_empty(), "Rendus trop différents de la référence : {echecs:?}");
}

fn lire_dossier(d: &Path, base: &Path, sortie: &mut Vec<Fichier>) {
    for e in std::fs::read_dir(d).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            lire_dossier(&p, base, sortie);
        } else {
            sortie.push(Fichier { chemin: p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"), octets: std::fs::read(&p).unwrap() });
        }
    }
}

#[test]
fn tous_les_exemples() {
    let mut jeux: Vec<(String, Vec<Fichier>)> = Vec::new();
    let ucamco = exemples().join("ucamco");
    let odb = exemples().join("odb");
    if !ucamco.exists() {
        eprintln!("Exemples absents : lancez scripts/telecharger-exemples.mjs");
        return;
    }
    // Les archives ZIP telles qu'elles ont été téléchargées : c'est ce que déposera l'utilisateur.
    for dossier in [&ucamco, &odb] {
        for e in std::fs::read_dir(dossier).unwrap() {
            let p = e.unwrap().path();
            let nom = p.file_name().unwrap().to_string_lossy().to_string();
            if p.is_file() && (nom.ends_with(".zip") || nom.ends_with(".tgz")) && !nom.starts_with("gerber-layer-format-test-files") {
                jeux.push((nom.clone(), vec![Fichier { chemin: nom, octets: std::fs::read(&p).unwrap() }]));
            }
        }
    }
    let _ = lire_dossier;
    let mut erreurs = Vec::new();
    for (nom, fichiers) in jeux {
        let debut = Instant::now();
        let mut lot = Lot { fichiers, ..Default::default() };
        let r = analyser(&mut lot);
        let duree = debut.elapsed().as_millis();
        let couches = r["couches"].as_array().cloned().unwrap_or_default();
        println!("\n== {nom} : {} couches en {duree} ms", couches.len());
        for c in &couches {
            println!("   {:42} {:10} {:8} objets  {}", c["nom"].as_str().unwrap_or(""), c["format"].as_str().unwrap_or(""), c["objets"], c["fichier"].as_str().unwrap_or(""));
            for a in c["meta"]["alertes"].as_array().into_iter().flatten() {
                println!("      ⚠ {a}");
            }
        }
        for a in r["alertes"].as_array().into_iter().flatten() {
            println!("   ⚠ {a}");
        }
        if let Some(o) = r["meta"]["odb"].as_object() {
            for a in o["alertes"].as_array().into_iter().flatten() {
                println!("   ⚠ ODB++ : {a}");
            }
            println!("   ODB++ : étape {}, {} composants, {} nets", o["etape"], r["meta"]["composants"].as_array().map(|a| a.len()).unwrap_or(0), r["meta"]["nets"].as_array().map(|a| a.len()).unwrap_or(0));
        } else if let Some(c) = r["meta"]["composants"].as_array() {
            println!("   {} composants (X3)", c.len());
        }
        let ignores = r["ignores"].as_array().map(|a| a.len()).unwrap_or(0);
        if ignores > 0 {
            println!("   fichiers ignorés : {}", r["ignores"]);
        }
        if couches.is_empty() && r["meta"]["job"].is_null() {
            erreurs.push(nom.clone());
        }
        // Chaque couche doit produire une géométrie non vide et se rendre sans paniquer.
        for (i, c) in lot.couches.iter().enumerate() {
            let (_, donnees) = typon_core::geometrie(&c.dessin);
            if donnees.is_empty() && !c.dessin.elements.is_empty() {
                erreurs.push(format!("{nom} : couche {i} sans géométrie"));
            }
        }
        // Rendu de contrôle de la première couche cuivre et du contour.
        let sortie = racine().join("essais/jeux");
        std::fs::create_dir_all(&sortie).unwrap();
        let boite = r["meta"]["boite"].as_array().map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect::<Vec<_>>()).unwrap_or_default();
        if boite.len() == 4 && boite[2] > boite[0] {
            let marge = (boite[2] - boite[0]).max(boite[3] - boite[1]) * 0.03;
            let cadre = [boite[0] - marge, boite[1] - marge, boite[2] + marge, boite[3] + marge];
            let l = 900usize;
            let h = ((cadre[3] - cadre[1]) / (cadre[2] - cadre[0]) * l as f64).round().clamp(10.0, 2000.0) as usize;
            for (i, c) in lot.couches.iter().enumerate().filter(|(_, c)| matches!(c.role.genre, typon_core::couches::Genre::Cuivre)).take(1) {
                let img = rendre(&c.dessin, l, h, cadre);
                ecrire_png(&sortie.join(format!("{nom}-{i}.png")), &img, l, h);
            }
        }
    }
    assert!(erreurs.is_empty(), "Jeux sans couche ou couches vides : {erreurs:?}");
}
