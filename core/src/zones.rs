//! Zones de différence d'une comparaison : à partir d'un masque (0 : identique, 1 : seulement dans A, 2 : seulement
//! dans B, 3 : les deux, à des endroits différents), regroupe les pixels voisins en zones et renvoie leurs rectangles.

use serde::Serialize;

#[derive(Serialize)]
pub struct Zone {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
    /// Pixels seulement dans A (disparus) et seulement dans B (apparus).
    pub a: u32,
    pub b: u32,
}

/// `fusion` : distance (en pixels) en dessous de laquelle deux différences forment une même zone. `minimum` : nombre
/// de pixels en dessous duquel une zone est ignorée (bruit d'anticrénelage).
pub fn zones(masque: &[u8], largeur: usize, hauteur: usize, fusion: usize, minimum: u32) -> Vec<Zone> {
    let pas = fusion.max(1);
    let gx = largeur.div_ceil(pas);
    let gy = hauteur.div_ceil(pas);
    let mut cases = vec![(0u32, 0u32); gx * gy];
    for y in 0..hauteur {
        let ligne = &masque[y * largeur..(y + 1) * largeur];
        for (x, &v) in ligne.iter().enumerate() {
            if v != 0 {
                let c = &mut cases[(y / pas) * gx + x / pas];
                if v & 1 != 0 {
                    c.0 += 1;
                }
                if v & 2 != 0 {
                    c.1 += 1;
                }
            }
        }
    }
    // Union-find sur les cases occupées, voisinage de 8.
    let mut parent: Vec<u32> = (0..(gx * gy) as u32).collect();
    fn racine(p: &mut [u32], mut i: u32) -> u32 {
        while p[i as usize] != i {
            p[i as usize] = p[p[i as usize] as usize];
            i = p[i as usize];
        }
        i
    }
    let occupe = |c: &(u32, u32)| c.0 + c.1 > 0;
    for j in 0..gy {
        for i in 0..gx {
            let k = j * gx + i;
            if !occupe(&cases[k]) {
                continue;
            }
            for (di, dj) in [(1i64, 0i64), (0, 1), (1, 1), (-1, 1)] {
                let (ni, nj) = (i as i64 + di, j as i64 + dj);
                if ni < 0 || nj < 0 || ni >= gx as i64 || nj >= gy as i64 {
                    continue;
                }
                let n = nj as usize * gx + ni as usize;
                if occupe(&cases[n]) {
                    let (ra, rb) = (racine(&mut parent, k as u32), racine(&mut parent, n as u32));
                    if ra != rb {
                        parent[ra as usize] = rb;
                    }
                }
            }
        }
    }
    let mut par_racine: std::collections::HashMap<u32, Zone> = std::collections::HashMap::new();
    for j in 0..gy {
        for i in 0..gx {
            let k = j * gx + i;
            if !occupe(&cases[k]) {
                continue;
            }
            let r = racine(&mut parent, k as u32);
            let (x0, y0) = ((i * pas) as u32, (j * pas) as u32);
            let (x1, y1) = ((((i + 1) * pas).min(largeur)) as u32, (((j + 1) * pas).min(hauteur)) as u32);
            let z = par_racine.entry(r).or_insert(Zone { x0, y0, x1, y1, a: 0, b: 0 });
            z.x0 = z.x0.min(x0);
            z.y0 = z.y0.min(y0);
            z.x1 = z.x1.max(x1);
            z.y1 = z.y1.max(y1);
            z.a += cases[k].0;
            z.b += cases[k].1;
        }
    }
    let mut r: Vec<Zone> = par_racine.into_values().filter(|z| z.a + z.b >= minimum).collect();
    r.sort_by(|p, q| (q.a + q.b).cmp(&(p.a + p.b)));
    r.truncate(1000);
    r
}
