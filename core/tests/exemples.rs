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
fn lire_reference(octets: &[u8]) -> (Vec<bool>, usize, usize) {
    let decodeur = png::Decoder::new(std::io::Cursor::new(octets.to_vec()));
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
    let zip = exemples().join("ucamco/gerber-layer-format-test-files.zip");
    let Ok(octets) = std::fs::read(&zip) else {
        eprintln!("Fichiers de test Ucamco absents : lancez node scripts/telecharger-exemples.mjs");
        return;
    };
    let fichiers = typon_core::archive::developper("tests.zip", octets).unwrap();
    let sortie = racine().join("essais/ucamco");
    std::fs::create_dir_all(&sortie).unwrap();
    let mut echecs = Vec::new();
    for f in fichiers.iter().filter(|f| f.chemin.ends_with(".gbr")) {
        let texte = String::from_utf8_lossy(&f.octets);
        let g = gerber::lire(&texte);
        let nom = f.chemin.rsplit('/').next().unwrap().to_string();
        let png_attendu = f.chemin.replace(".gbr", ".png");
        let Some(reference) = fichiers.iter().find(|x| x.chemin == png_attendu) else {
            println!("{nom:45} {} objets, sans image de référence", g.dessin.objets.len());
            continue;
        };
        let (refimg, l, h) = lire_reference(&reference.octets);
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

/// Cartes d'exemple du site (générées par scripts/generer_exemples.py) : toujours présentes, essai indispensable.
#[test]
fn exemples_du_site() {
    let dossier = racine().join("public/exemples");
    // Révision B : LED d'alimentation D2 et sa résistance R11 retirées, condensateur C13 ajouté.
    for (nom, couches_min, composants) in [("typon-demo-revA.zip", 13, 47), ("typon-demo-revB.zip", 13, 46), ("typon-demo-revA-odb.tgz", 12, 47)] {
        let octets = std::fs::read(dossier.join(nom)).expect("exemple absent : lancez python scripts/generer_exemples.py");
        let mut lot = Lot { fichiers: vec![Fichier { chemin: nom.into(), octets }], ..Default::default() };
        let r = analyser(&mut lot);
        let n = r["couches"].as_array().map(|a| a.len()).unwrap_or(0);
        assert!(n >= couches_min, "{nom} : {n} couches");
        assert_eq!(r["meta"]["composants"].as_array().map(|a| a.len()).unwrap_or(0), composants, "{nom} : composants");
        let b = r["meta"]["boite"].as_array().unwrap();
        let (l, h) = (b[2].as_f64().unwrap() - b[0].as_f64().unwrap(), b[3].as_f64().unwrap() - b[1].as_f64().unwrap());
        assert!((l - 60.0).abs() < 0.01 && (h - 38.0).abs() < 0.01, "{nom} : dimensions {l} × {h}");
        let nets = r["meta"]["nets"].as_array().map(|a| a.len()).unwrap_or(0);
        assert!(nets >= 35, "{nom} : {nets} nets");
        assert!(r["alertes"].as_array().map(|a| a.is_empty()).unwrap_or(true), "{nom} : alertes {}", r["alertes"]);
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

/// Fichiers IPC-2581 (cas de test du consortium, exports KiCad et Altium) : lecture, résumé, rendu de contrôle.
#[test]
fn ipc2581() {
    let dossier = exemples().join("ipc2581");
    if !dossier.exists() {
        eprintln!("Exemples IPC-2581 absents : lancez node scripts/telecharger-exemples.mjs");
        return;
    }
    let mut chemins: Vec<PathBuf> = Vec::new();
    fn parcourir(d: &Path, sortie: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                parcourir(&p, sortie);
            } else if p.extension().map(|x| x == "xml" || x == "cvg" || x == "zip").unwrap_or(false) {
                sortie.push(p);
            }
        }
    }
    parcourir(&dossier, &mut chemins);
    chemins.sort();
    let sortie = racine().join("essais/ipc2581");
    std::fs::create_dir_all(&sortie).unwrap();
    let mut erreurs = Vec::new();
    for chemin in chemins {
        let nom = chemin.file_name().unwrap().to_string_lossy().to_string();
        let debut = Instant::now();
        let mut lot = Lot { fichiers: vec![Fichier { chemin: nom.clone(), octets: std::fs::read(&chemin).unwrap() }], ..Default::default() };
        let r = analyser(&mut lot);
        let duree = debut.elapsed().as_millis();
        let couches = r["couches"].as_array().cloned().unwrap_or_default();
        let comp = r["meta"]["composants"].as_array().map(|a| a.len()).unwrap_or(0);
        let nets = r["meta"]["nets"].as_array().map(|a| a.len()).unwrap_or(0);
        let objets: u64 = couches.iter().map(|c| c["objets"].as_u64().unwrap_or(0)).sum();
        println!("{nom:45} {:3} couches, {objets:7} objets, {comp:4} composants, {nets:4} nets, {duree:5} ms", couches.len());
        for a in r["alertes"].as_array().into_iter().flatten().chain(r["meta"]["odb"]["alertes"].as_array().into_iter().flatten()) {
            println!("      ⚠ {a}");
        }
        if nom.contains("Stack") || nom.contains("BOM") {
            continue; // fichiers d'empilage ou de nomenclature seuls : pas de dessin attendu
        }
        if couches.is_empty() || objets == 0 {
            erreurs.push(nom.clone());
            continue;
        }
        let boite = r["meta"]["boite"].as_array().map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect::<Vec<_>>()).unwrap_or_default();
        if boite.len() == 4 && boite[2] > boite[0] {
            let marge = (boite[2] - boite[0]).max(boite[3] - boite[1]) * 0.03;
            let cadre = [boite[0] - marge, boite[1] - marge, boite[2] + marge, boite[3] + marge];
            let l = 900usize;
            let h = ((cadre[3] - cadre[1]) / (cadre[2] - cadre[0]) * l as f64).round().clamp(10.0, 2000.0) as usize;
            if let Some(c) = lot.couches.iter().filter(|c| matches!(c.role.genre, typon_core::couches::Genre::Cuivre)).max_by_key(|c| c.dessin.elements.len()) {
                let img = rendre(&c.dessin, l, h, cadre);
                ecrire_png(&sortie.join(format!("{nom}.png")), &img, l, h);
            }
        }
    }
    assert!(erreurs.is_empty(), "Fichiers IPC-2581 sans dessin : {erreurs:?}");
}

/// Boîte et nombre d'objets d'un dessin, sans les régions (zones de cuivre, que le RS-274D ne sait pas décrire).
fn hors_regions(d: &Dessin) -> (typon_core::geom::BBox, usize) {
    let region = |objet: u32| d.info_objet(objet).map(|i| i.iter().any(|(k, v)| k == "Objet" && v == "Région")).unwrap_or(false);
    let mut b = typon_core::geom::BBox::vide();
    for e in d.elements.iter().filter(|e| !region(e.objet)) {
        b.union(&e.prim.boite_englobante());
    }
    (b, (0..d.objets.len() as u32).filter(|&o| !region(o)).count())
}

/// Gerber RS-274D avec liste d'ouvertures séparée (roue Eagle en pouces, liste Protel en mils et zéros de fin omis) :
/// même dessin que la version Gerber X2 de la carte d'exemple (pistes, pastilles et vias ; le RS-274D n'a pas de
/// régions, donc pas de zones de cuivre).
#[test]
fn rs274d_avec_liste_d_ouvertures() {
    let reference = {
        let mut lot = Lot { fichiers: vec![Fichier { chemin: "a.zip".into(), octets: std::fs::read(racine().join("public/exemples/typon-demo-revA.zip")).unwrap() }], ..Default::default() };
        analyser(&mut lot);
        let c = lot.couches.iter().find(|c| c.nom == "Cuivre dessus").unwrap();
        hors_regions(&c.dessin)
    };
    for nom in ["rs274d-eagle.zip", "rs274d-protel.zip"] {
        let octets = std::fs::read(racine().join("core/tests/donnees").join(nom)).unwrap();
        let mut lot = Lot { fichiers: vec![Fichier { chemin: nom.into(), octets }], ..Default::default() };
        let r = analyser(&mut lot);
        let couches = r["couches"].as_array().unwrap();
        let cuivre = couches.iter().find(|c| c["nom"] == "Cuivre dessus").unwrap_or_else(|| panic!("{nom} : pas de cuivre dessus : {r}"));
        assert_eq!(cuivre["meta"]["format"], "Gerber RS-274D", "{nom}");
        let alerte = cuivre["meta"]["alertes"][0].as_str().unwrap_or("");
        assert!(alerte.contains("typon-demo."), "{nom} : {alerte}");
        let c = lot.couches.iter().find(|c| c.nom == "Cuivre dessus").unwrap();
        let b = c.dessin.boite();
        let ecart = (b.x0 - reference.0.x0).abs().max((b.x1 - reference.0.x1).abs()).max((b.y0 - reference.0.y0).abs()).max((b.y1 - reference.0.y1).abs());
        println!("{nom} : {} objets (X2 : {}), écart de la boîte {ecart:.4} mm ; {alerte}", c.dessin.objets.len(), reference.1);
        assert!(ecart < 0.02, "{nom} : boîte {:?} au lieu de {:?}", b, reference.0);
        assert_eq!(c.dessin.objets.len(), reference.1, "{nom} : nombre d'objets");
        assert!(couches.iter().any(|c| c["nom"] == "Sérigraphie dessus"), "{nom} : sérigraphie");
    }
}

#[test]
fn listes_d_ouvertures() {
    use typon_core::ouvertures::{Forme, lire};
    let generique = "D-code  Forme      Dimensions\nD10     CIRCLE     10 mil\nD11     RECT       60x40 mil\nD12     OBLONG     50 x 80 mil\nD13     DONUT      60 30 mil\n";
    let l = lire(generique).unwrap();
    assert_eq!(l.unite, "mils");
    assert_eq!(l.ouvertures.len(), 4);
    assert_eq!(l.ouvertures[&11].forme, Forme::Rectangle);
    assert!((l.ouvertures[&11].a - 1.524).abs() < 1e-9 && (l.ouvertures[&11].b - 1.016).abs() < 1e-9);
    assert_eq!(l.ouvertures[&13].forme, Forme::Anneau);
    let ad = "%MOMM*%\n%ADD10C,0.25*%\n%ADD11R,1.2X0.6*%\n";
    let l = lire(ad).unwrap();
    assert_eq!(l.unite, "mm");
    assert!((l.ouvertures[&11].a - 1.2).abs() < 1e-9);
}

#[test]
fn code_barres_odb() {
    // Code 39 de « TD-0001 » avec clé : 10 caractères de 15 modules et 9 espaces, clé « - » (somme 79 mod 43 = 36).
    let m = typon_core::odb::code39("TD-0001", false, true);
    assert_eq!(m.len(), 159);
    let m2 = typon_core::odb::code39("TD-0001-", false, false);
    assert_eq!(m, m2, "la clé de contrôle de TD-0001 est « - »");
    // Le code-barres de la carte d'exemple ODB++ est dessiné dans la sérigraphie du dessus.
    let octets = std::fs::read(racine().join("public/exemples/typon-demo-revA-odb.tgz")).unwrap();
    let mut lot = Lot { fichiers: vec![Fichier { chemin: "odb.tgz".into(), octets }], ..Default::default() };
    analyser(&mut lot);
    let c = lot.couches.iter().find(|c| c.nom == "Sérigraphie dessus").unwrap();
    let info = c.dessin.infos.iter().find(|i| i.iter().any(|(k, _)| k == "Code-barres")).expect("objet code-barres");
    println!("{info:?}");
    let barres = c.dessin.elements.iter().filter(|e| matches!(e.prim, typon_core::geom::Prim::Boite { .. })).count();
    assert!(barres >= 50, "{barres} barres");
}

/// DFM sur la carte d'exemple (Gerber et ODB++) et sur les exemples externes présents : durée et nombre de résultats
/// par règle. La carte d'exemple doit passer les règles d'un fabricant courant sans court-circuit.
#[test]
fn dfm() {
    let regles = typon_core::dfm::Regles::depuis(&serde_json::json!({}));
    let mut jeux: Vec<(String, PathBuf)> = vec![
        ("demo-gerber".into(), racine().join("public/exemples/typon-demo-revA.zip")),
        ("demo-odb".into(), racine().join("public/exemples/typon-demo-revA-odb.tgz")),
    ];
    for f in ["ipc2581/royalblue54l-feather.xml", "odb/sandbox-odb_wifi.tgz", "odb/designodb_rigidflex.tgz", "ucamco/gerber-layer-format-x3-li-ion-charger.zip"] {
        let p = exemples().join(f);
        if p.exists() {
            jeux.push((f.into(), p));
        }
    }
    for (nom, chemin) in jeux {
        let octets = std::fs::read(&chemin).unwrap();
        let mut lot = Lot { fichiers: vec![Fichier { chemin: chemin.file_name().unwrap().to_string_lossy().into(), octets }], ..Default::default() };
        analyser(&mut lot);
        let t0 = Instant::now();
        let r = typon_core::dfm::verifier(&lot.couches, &regles);
        println!("DFM {nom} : {:.2} s, vérifiées {}, comptes {}", t0.elapsed().as_secs_f64(), r["verifiees"], r["comptes"]);
        for x in r["resultats"].as_array().unwrap().iter().filter(|x| nom.starts_with("demo") || x["regle"] == "court" || x["regle"] == "isolement").take(12) {
            println!("   {x}");
        }
        if nom.starts_with("demo") {
            assert!(r["verifiees"].as_array().unwrap().len() >= 8, "{nom} : règles vérifiées {}", r["verifiees"]);
            // Exactement les six défauts volontaires de docs/defauts-exemple.md (anneau et bord : sur les deux faces).
            assert_eq!(r["comptes"], serde_json::json!({"anneau": 2, "cuivreBord": 2, "isolement": 1, "percage": 1, "piste": 1, "serigraphiePastille": 1}), "{nom}");
            for (regle, x, y) in [("piste", 52.35, 30.94), ("isolement", 35.0, 5.55), ("anneau", 22.8, 19.0), ("cuivreBord", 30.0, 0.55), ("percage", 40.5, 14.5), ("serigraphiePastille", 14.2, 6.85)] {
                let trouve = r["resultats"].as_array().unwrap().iter().any(|d| d["regle"] == regle && (d["x"].as_f64().unwrap() - x).hypot(d["y"].as_f64().unwrap() - y) < 1.0);
                assert!(trouve, "{nom} : défaut {regle} attendu vers ({x} ; {y})");
            }
        }
    }
}

/// Panneau 3 × 2 de la carte d'exemple : relu par le moteur, il doit donner les mêmes couches, aux bonnes dimensions,
/// avec six fois les trous de la carte (plus les trous du panneau).
#[test]
fn panneau() {
    let octets = std::fs::read(racine().join("public/exemples/typon-demo-revA.zip")).unwrap();
    let mut lot = Lot { fichiers: vec![Fichier { chemin: "demo.zip".into(), octets }], ..Default::default() };
    let r = analyser(&mut lot);
    let n_couches = r["couches"].as_array().unwrap().len();
    for separation in ["fraisage", "vscore"] {
        let p = serde_json::json!({"nx": 3, "ny": 2, "separation": separation, "bord": 5, "bords": "haut-bas"});
        let (j, zip) = typon_core::fabrication::panneau(&lot.couches, &p).unwrap();
        println!("Panneau {separation} : {}", j);
        let mut lot2 = Lot { fichiers: vec![Fichier { chemin: "panneau.zip".into(), octets: zip.clone() }], ..Default::default() };
        std::fs::write(racine().join(format!("essais/panneau-{separation}.zip")), &zip).ok();
        let r2 = analyser(&mut lot2);
        let n2 = r2["couches"].as_array().unwrap().len();
        assert!(n2 + 2 >= n_couches, "{separation} : {n2} couches relues pour {n_couches}");
        assert!(r2["alertes"].as_array().map(|a| a.is_empty()).unwrap_or(true), "{separation} : alertes {}", r2["alertes"]);
        let b = r2["meta"]["boite"].as_array().unwrap();
        let (l, h) = (b[2].as_f64().unwrap() - b[0].as_f64().unwrap(), b[3].as_f64().unwrap() - b[1].as_f64().unwrap());
        assert!((l - j["largeur"].as_f64().unwrap()).abs() < 0.2 && (h - j["hauteur"].as_f64().unwrap()).abs() < 0.2, "{separation} : {l} × {h} pour {}", j);
        let trous = |r: &serde_json::Value| r["couches"].as_array().unwrap().iter().filter(|c| c["role"]["genre"] == "percage").map(|c| c["meta"]["trous"].as_u64().unwrap_or(0)).sum::<u64>();
        assert!(trous(&r2) >= 6 * trous(&r), "{separation} : {} trous pour {}", trous(&r2), trous(&r));
        // Le cuivre du dessus : six fois les objets de la carte (et les mires).
        let objets = |r: &serde_json::Value| r["couches"].as_array().unwrap().iter().find(|c| c["role"]["genre"] == "cuivre" && c["role"]["cote"] == "dessus").map(|c| c["objets"].as_u64().unwrap_or(0)).unwrap_or(0);
        println!("  cuivre dessus : {} objets pour {} ; trous {} pour {}", objets(&r2), objets(&r), trous(&r2), trous(&r));
        // Contour du panneau : bord extérieur et découpes (canaux de fraisage, découpe en L de chaque carte).
        let contour = lot2.couches.iter().find(|c| c.role.genre == typon_core::couches::Genre::Contour).expect("contour du panneau");
        let anneaux = typon_core::geom::anneaux_contour(&contour.dessin);
        println!("  contour : {} anneaux fermés", anneaux.len());
        if separation == "fraisage" {
            // Le panneau, et pour chaque carte au moins un canal de fraisage et sa découpe en L.
            assert!(anneaux.len() > 6, "fraisage : {} anneaux seulement", anneaux.len());
        } else {
            // Le panneau et les six découpes en L.
            assert!(anneaux.len() >= 7, "rainures en V : {} anneaux", anneaux.len());
            assert!(lot2.couches.iter().any(|c| c.fichier.contains("rainurage")), "fichier des rainures en V absent");
        }
        // Mires et trous d'outillage : au moins 3 mm d'écart.
        let gros: Vec<_> = lot2.couches.iter().filter(|c| c.role.genre == typon_core::couches::Genre::Percage).flat_map(|c| c.dessin.elements.iter()).filter_map(|e| match e.prim { typon_core::geom::Prim::Cap { a, r, .. } if (r - 1.5).abs() < 0.01 => Some(a), _ => None }).collect();
        let mires: Vec<_> = lot2.couches.iter().filter(|c| c.role.genre == typon_core::couches::Genre::Cuivre).flat_map(|c| c.dessin.info_objet(0).map(|_| c)).flat_map(|c| c.dessin.elements.iter().filter(|e| c.dessin.info_objet(e.objet).map(|i| i.iter().any(|(_, v)| v.contains("Fiducial"))).unwrap_or(false))).filter_map(|e| match e.prim { typon_core::geom::Prim::Cap { a, .. } => Some(a), _ => None }).collect();
        assert_eq!(gros.len(), 4, "{separation} : trous d'outillage");
        for m in &mires {
            for t in &gros {
                assert!(m.dist(*t) > 3.0, "{separation} : mire à {:.2} mm d'un trou d'outillage", m.dist(*t));
            }
        }
        // Le DFM du panneau ne doit pas trouver de court-circuit (régions réécrites sans croisement).
        let d = typon_core::dfm::verifier(&lot2.couches, &typon_core::dfm::Regles::depuis(&serde_json::json!({})));
        assert!(d["comptes"].get("court").is_none(), "{separation} : courts-circuits dans le panneau {}", d["comptes"]);
        println!("  DFM du panneau : {}", d["comptes"]);
    }
}
