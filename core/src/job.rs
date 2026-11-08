//! Fichier de job Gerber (.gbrjob, JSON) : informations générales de fabrication, empilage, rôle de chaque fichier.

use serde_json::Value;

pub struct Job {
    /// Rubriques lisibles : (titre, [(libellé, valeur)]).
    pub rubriques: Vec<(String, Vec<(String, String)>)>,
    /// Fichiers décrits : chemin -> .FileFunction (valeurs séparées).
    pub fichiers: Vec<(String, Vec<String>)>,
    /// Empilage : (couche, type, épaisseur en mm, matériau)
    pub empilage: Vec<(String, String, Option<f64>, String)>,
}

pub fn lire(texte: &str) -> Result<Job, String> {
    let v: Value = serde_json::from_str(texte).map_err(|e| format!("Fichier de job illisible : {e}"))?;
    let mut rubriques = Vec::new();
    let texte_de = |v: &Value| -> String {
        match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string().replace('.', ","),
            Value::Bool(b) => if *b { "oui".into() } else { "non".into() },
            Value::Array(a) => a.iter().map(|x| match x { Value::String(s) => s.clone(), autre => autre.to_string() }).collect::<Vec<_>>().join(", "),
            Value::Object(o) => o.iter().map(|(k, v)| format!("{k} : {}", match v { Value::String(s) => s.clone(), autre => autre.to_string() })).collect::<Vec<_>>().join(" ; "),
            Value::Null => String::new(),
        }
    };
    for (cle, titre) in [("Header", "En-tête"), ("GeneralSpecs", "Caractéristiques générales"), ("DesignRules", "Règles de conception")] {
        if let Some(o) = v.get(cle) {
            let mut lignes = Vec::new();
            aplatir(o, "", &mut lignes, &texte_de);
            if !lignes.is_empty() {
                rubriques.push((titre.to_string(), lignes.into_iter().map(|(k, v)| (libelle(&k), v)).collect()));
            }
        }
    }
    let mut fichiers = Vec::new();
    if let Some(Value::Array(a)) = v.get("FilesAttributes") {
        for f in a {
            let chemin = f.get("Path").and_then(|p| p.as_str()).unwrap_or("").to_string();
            let fonction = f.get("FileFunction").and_then(|p| p.as_str()).unwrap_or("").split(',').map(|s| s.to_string()).collect();
            fichiers.push((chemin, fonction));
        }
    }
    let mut empilage = Vec::new();
    if let Some(Value::Array(a)) = v.get("MaterialStackup") {
        for c in a {
            empilage.push((
                c.get("Name").and_then(|p| p.as_str()).unwrap_or("").to_string(),
                c.get("Type").and_then(|p| p.as_str()).unwrap_or("").to_string(),
                c.get("Thickness").and_then(|p| p.as_f64()),
                c.get("Material").and_then(|p| p.as_str()).unwrap_or("").to_string(),
            ));
        }
    }
    Ok(Job { rubriques, fichiers, empilage })
}

fn aplatir(v: &Value, prefixe: &str, sortie: &mut Vec<(String, String)>, texte_de: &dyn Fn(&Value) -> String) {
    match v {
        Value::Object(o) => {
            for (k, x) in o {
                let nom = if prefixe.is_empty() { k.clone() } else { format!("{prefixe} · {k}") };
                match x {
                    Value::Object(_) => aplatir(x, &nom, sortie, texte_de),
                    _ => sortie.push((nom, texte_de(x))),
                }
            }
        }
        autre => sortie.push((prefixe.to_string(), texte_de(autre))),
    }
}

fn libelle(k: &str) -> String {
    let traduire = |p: &str| -> String {
        match p {
            "GenerationSoftware" => "Logiciel".into(),
            "Vendor" => "Éditeur".into(),
            "Application" => "Application".into(),
            "Version" => "Version".into(),
            "CreationDate" => "Date de création".into(),
            "ProjectId" => "Projet".into(),
            "Name" => "Nom".into(),
            "GUID" => "Identifiant".into(),
            "Revision" => "Révision".into(),
            "Size" => "Taille (mm)".into(),
            "X" => "largeur".into(),
            "Y" => "hauteur".into(),
            "LayerNumber" => "Nombre de couches".into(),
            "BoardThickness" => "Épaisseur (mm)".into(),
            "Finish" => "Finition".into(),
            "ImpedanceControlled" => "Impédance contrôlée".into(),
            "Legend" => "Sérigraphie".into(),
            "SolderMask" => "Masque de soudure".into(),
            "Color" => "couleur".into(),
            "Top" => "dessus".into(),
            "Bottom" => "dessous".into(),
            "Layers" => "Couches".into(),
            "PadToPad" => "Écart pastille-pastille".into(),
            "PadToTrack" => "Écart pastille-piste".into(),
            "TrackToTrack" => "Écart piste-piste".into(),
            "MinLineWidth" => "Largeur de piste minimale".into(),
            "TrackToRegion" => "Écart piste-zone".into(),
            "RegionToRegion" => "Écart zone-zone".into(),
            "BoardOutline" => "Contour".into(),
            "Polarity" => "Polarité".into(),
            "IPC-6012Class" | "IPCClass" => "Classe IPC".into(),
            autre => autre.to_string(),
        }
    };
    k.split(" · ").map(traduire).collect::<Vec<_>>().join(" · ")
}
