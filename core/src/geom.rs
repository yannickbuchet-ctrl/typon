//! Géométrie commune à tous les formats : points, transformations, primitives de dessin, et « image » d'une couche
//! prête pour WebGL (lots de primitives regroupées par polarité, dans l'ordre du fichier).
//!
//! Unités : millimètres, repère du fichier (y vers le haut). Angles en radians, sens trigonométrique.

use std::collections::HashMap;
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

pub const fn pt(x: f64, y: f64) -> Pt {
    Pt { x, y }
}

impl Pt {
    pub fn dist(self, o: Pt) -> f64 {
        ((self.x - o.x).powi(2) + (self.y - o.y).powi(2)).sqrt()
    }
}

/// Transformation affine : x' = a·x + c·y + e ; y' = b·x + d·y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transfo {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Default for Transfo {
    fn default() -> Self {
        Transfo::IDENTITE
    }
}

impl Transfo {
    pub const IDENTITE: Transfo = Transfo { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn translation(x: f64, y: f64) -> Transfo {
        Transfo { e: x, f: y, ..Transfo::IDENTITE }
    }
    pub fn rotation(angle: f64) -> Transfo {
        let (s, c) = angle.sin_cos();
        Transfo { a: c, b: s, c: -s, d: c, e: 0.0, f: 0.0 }
    }
    pub fn echelle(k: f64) -> Transfo {
        Transfo { a: k, d: k, ..Transfo::IDENTITE }
    }
    /// Miroir : mx inverse les x, my inverse les y.
    pub fn miroir(mx: bool, my: bool) -> Transfo {
        Transfo { a: if mx { -1.0 } else { 1.0 }, d: if my { -1.0 } else { 1.0 }, ..Transfo::IDENTITE }
    }
    /// self ∘ autre : applique d'abord `autre`, puis `self`.
    pub fn apres(&self, autre: &Transfo) -> Transfo {
        Transfo {
            a: self.a * autre.a + self.c * autre.b,
            b: self.b * autre.a + self.d * autre.b,
            c: self.a * autre.c + self.c * autre.d,
            d: self.b * autre.c + self.d * autre.d,
            e: self.a * autre.e + self.c * autre.f + self.e,
            f: self.b * autre.e + self.d * autre.f + self.f,
        }
    }
    pub fn applique(&self, p: Pt) -> Pt {
        pt(self.a * p.x + self.c * p.y + self.e, self.b * p.x + self.d * p.y + self.f)
    }
    /// Facteur d'échelle (moyenne géométrique des axes).
    pub fn echelle_moyenne(&self) -> f64 {
        (self.a * self.d - self.b * self.c).abs().sqrt()
    }
    pub fn est_identite(&self) -> bool {
        *self == Transfo::IDENTITE
    }
}

/// Une primitive de dessin.
#[derive(Clone, Debug)]
pub enum Prim {
    /// Segment à bouts ronds (a = b : disque). r = demi-largeur ; r = 0 : trait fin (un pixel à l'écran).
    Cap { a: Pt, b: Pt, r: f64 },
    /// Rectangle centré, demi-côtés hw et hh, tourné de `ang`, coins arrondis de rayon `rad`.
    Boite { c: Pt, hw: f64, hh: f64, ang: f64, rad: f64 },
    /// Polygone : premier anneau = contour extérieur, les suivants = trous. Rempli pair-impair.
    Poly { anneaux: Vec<Vec<Pt>> },
}

impl Prim {
    pub fn transformee(&self, t: &Transfo) -> Prim {
        if t.est_identite() {
            return self.clone();
        }
        let k = t.echelle_moyenne();
        match self {
            Prim::Cap { a, b, r } => Prim::Cap { a: t.applique(*a), b: t.applique(*b), r: r * k },
            Prim::Boite { c, hw, hh, ang, rad } => {
                let (s, co) = ang.sin_cos();
                let dx = t.a * co + t.c * s;
                let dy = t.b * co + t.d * s;
                Prim::Boite { c: t.applique(*c), hw: hw * k, hh: hh * k, ang: dy.atan2(dx), rad: rad * k }
            }
            Prim::Poly { anneaux } => Prim::Poly {
                anneaux: anneaux.iter().map(|a| a.iter().map(|p| t.applique(*p)).collect()).collect(),
            },
        }
    }

    pub fn boite_englobante(&self) -> BBox {
        let mut b = BBox::vide();
        match self {
            Prim::Cap { a, b: q, r } => {
                b.ajoute(*a);
                b.ajoute(*q);
                b.gonfle(*r);
            }
            Prim::Boite { c, hw, hh, ang, .. } => {
                let (s, co) = ang.sin_cos();
                let ex = (hw * co).abs() + (hh * s).abs();
                let ey = (hw * s).abs() + (hh * co).abs();
                b.ajoute(pt(c.x - ex, c.y - ey));
                b.ajoute(pt(c.x + ex, c.y + ey));
            }
            Prim::Poly { anneaux } => {
                for a in anneaux {
                    for p in a {
                        b.ajoute(*p);
                    }
                }
            }
        }
        b
    }

    /// Le point est-il dans la primitive (à `tol` près) ?
    pub fn contient(&self, p: Pt, tol: f64) -> bool {
        match self {
            Prim::Cap { a, b, r } => dist_segment(p, *a, *b) <= r + tol,
            Prim::Boite { c, hw, hh, ang, rad } => {
                let (s, co) = ang.sin_cos();
                let dx = p.x - c.x;
                let dy = p.y - c.y;
                let lx = (dx * co + dy * s).abs();
                let ly = (-dx * s + dy * co).abs();
                let qx = lx - (hw - rad);
                let qy = ly - (hh - rad);
                let ext = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0);
                ext - rad <= tol
            }
            Prim::Poly { anneaux } => {
                let mut dedans = false;
                for a in anneaux {
                    if point_dans_anneau(p, a) {
                        dedans = !dedans;
                    }
                }
                dedans || (tol > 0.0 && anneaux.iter().any(|a| dist_anneau(p, a) <= tol))
            }
        }
    }
}

pub fn dist_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let (vx, vy) = (b.x - a.x, b.y - a.y);
    let l2 = vx * vx + vy * vy;
    let t = if l2 > 0.0 { (((p.x - a.x) * vx + (p.y - a.y) * vy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    p.dist(pt(a.x + t * vx, a.y + t * vy))
}

fn dist_anneau(p: Pt, a: &[Pt]) -> f64 {
    let mut d = f64::INFINITY;
    for i in 0..a.len() {
        d = d.min(dist_segment(p, a[i], a[(i + 1) % a.len()]));
    }
    d
}

pub fn point_dans_anneau(p: Pt, a: &[Pt]) -> bool {
    let mut dedans = false;
    let n = a.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (pi, pj) = (a[i], a[j]);
        if (pi.y > p.y) != (pj.y > p.y) && p.x < (pj.x - pi.x) * (p.y - pi.y) / (pj.y - pi.y) + pi.x {
            dedans = !dedans;
        }
        j = i;
    }
    dedans
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BBox {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl BBox {
    pub fn vide() -> BBox {
        BBox { x0: f64::INFINITY, y0: f64::INFINITY, x1: f64::NEG_INFINITY, y1: f64::NEG_INFINITY }
    }
    pub fn est_vide(&self) -> bool {
        self.x0 > self.x1
    }
    pub fn ajoute(&mut self, p: Pt) {
        self.x0 = self.x0.min(p.x);
        self.y0 = self.y0.min(p.y);
        self.x1 = self.x1.max(p.x);
        self.y1 = self.y1.max(p.y);
    }
    pub fn union(&mut self, o: &BBox) {
        if !o.est_vide() {
            self.x0 = self.x0.min(o.x0);
            self.y0 = self.y0.min(o.y0);
            self.x1 = self.x1.max(o.x1);
            self.y1 = self.y1.max(o.y1);
        }
    }
    pub fn gonfle(&mut self, m: f64) {
        if !self.est_vide() {
            self.x0 -= m;
            self.y0 -= m;
            self.x1 += m;
            self.y1 += m;
        }
    }
    pub fn contient(&self, p: Pt, tol: f64) -> bool {
        p.x >= self.x0 - tol && p.x <= self.x1 + tol && p.y >= self.y0 - tol && p.y <= self.y1 + tol
    }
    pub fn tableau(&self) -> [f64; 4] {
        if self.est_vide() { [0.0; 4] } else { [self.x0, self.y0, self.x1, self.y1] }
    }
}

/// Points d'un arc de `debut` à `fin` autour de `centre` (sens horaire si `horaire`), extrémités comprises. Un arc dont
/// le début et la fin coïncident est un cercle complet.
pub fn arc(debut: Pt, fin: Pt, centre: Pt, horaire: bool) -> Vec<Pt> {
    let r0 = debut.dist(centre);
    let r1 = fin.dist(centre);
    let a0 = (debut.y - centre.y).atan2(debut.x - centre.x);
    let a1 = (fin.y - centre.y).atan2(fin.x - centre.x);
    let mut balayage = if horaire { a0 - a1 } else { a1 - a0 };
    while balayage < 0.0 {
        balayage += 2.0 * PI;
    }
    if balayage < 1e-9 && debut.dist(fin) < 1e-9 {
        balayage = 2.0 * PI;
    }
    let r = r0.max(r1);
    let n = segments_cercle(r, balayage);
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let ang = if horaire { a0 - balayage * t } else { a0 + balayage * t };
        let rr = r0 + (r1 - r0) * t;
        pts.push(pt(centre.x + rr * ang.cos(), centre.y + rr * ang.sin()));
    }
    pts[0] = debut;
    *pts.last_mut().unwrap() = fin;
    pts
}

/// Nombre de segments pour approcher un arc de rayon r et d'angle `balayage` à ~1 µm près.
pub fn segments_cercle(r: f64, balayage: f64) -> usize {
    let tol = 0.001_f64;
    let pas = if r > tol { 2.0 * (1.0 - tol / r).clamp(-1.0, 1.0).acos() } else { PI / 4.0 };
    let n = (balayage / pas.max(1e-3)).ceil() as usize;
    let mini = ((balayage / (2.0 * PI)) * 16.0).ceil() as usize;
    n.clamp(mini.max(1), 512)
}

pub fn cercle(c: Pt, r: f64) -> Vec<Pt> {
    let n = segments_cercle(r, 2.0 * PI);
    (0..n).map(|i| {
        let a = 2.0 * PI * i as f64 / n as f64;
        pt(c.x + r * a.cos(), c.y + r * a.sin())
    }).collect()
}

/// Polygone régulier à n sommets, de diamètre extérieur d, premier sommet à l'angle `rot`.
pub fn polygone_regulier(c: Pt, d: f64, n: usize, rot: f64) -> Vec<Pt> {
    let n = n.max(3);
    (0..n).map(|i| {
        let a = rot + 2.0 * PI * i as f64 / n as f64;
        pt(c.x + d / 2.0 * a.cos(), c.y + d / 2.0 * a.sin())
    }).collect()
}

/// Rectangle à coins arrondis (rad) ou chanfreinés (chanfrein), coins choisis par `coins` (bits 1 à 4 : haut droit,
/// haut gauche, bas gauche, bas droit, comme ODB++), en polygone.
pub fn rectangle_coins(c: Pt, w: f64, h: f64, rad: f64, chanfrein: bool, coins: u8) -> Vec<Pt> {
    let (hw, hh) = (w / 2.0, h / 2.0);
    let r = rad.min(hw).min(hh).max(0.0);
    let centres = [(hw - r, hh - r, 0.0), (-hw + r, hh - r, PI / 2.0), (-hw + r, -hh + r, PI), (hw - r, -hh + r, 1.5 * PI)];
    let angles = [(hw, hh), (-hw, hh), (-hw, -hh), (hw, -hh)];
    let mut pts = Vec::new();
    for (i, (cx, cy, a0)) in centres.iter().enumerate() {
        let actif = coins & (1 << i) != 0 && r > 0.0;
        if !actif {
            pts.push(pt(c.x + angles[i].0, c.y + angles[i].1));
        } else if chanfrein {
            pts.push(pt(c.x + cx + r * a0.cos(), c.y + cy + r * a0.sin()));
            let a1 = a0 + PI / 2.0;
            pts.push(pt(c.x + cx + r * a1.cos(), c.y + cy + r * a1.sin()));
        } else {
            let n = segments_cercle(r, PI / 2.0);
            for k in 0..=n {
                let a = a0 + PI / 2.0 * k as f64 / n as f64;
                pts.push(pt(c.x + cx + r * a.cos(), c.y + cy + r * a.sin()));
            }
        }
    }
    // Les coins chanfreinés ou arrondis partent du côté droit : l'ordre ci-dessus suit le sens trigonométrique.
    pts
}

/// Coupe un polygone convexe par le demi-plan n·p ≤ k (Sutherland-Hodgman).
pub fn couper_demi_plan(poly: &[Pt], nx: f64, ny: f64, k: f64) -> Vec<Pt> {
    let mut r = Vec::with_capacity(poly.len() + 2);
    let n = poly.len();
    for i in 0..n {
        let p = poly[i];
        let q = poly[(i + 1) % n];
        let dp = nx * p.x + ny * p.y - k;
        let dq = nx * q.x + ny * q.y - k;
        if dp <= 0.0 {
            r.push(p);
        }
        if (dp <= 0.0) != (dq <= 0.0) {
            let t = dp / (dp - dq);
            r.push(pt(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t));
        }
    }
    r
}

/// Pastille thermique : la couronne entre `exterieur` et `interieur` (polygones convexes centrés sur `c`), coupée par
/// `branches` bras d'épaisseur `ecart`, le premier à l'angle `angle`. Renvoie un polygone (avec trou) par secteur.
pub fn thermique(c: Pt, exterieur: &[Pt], interieur: &[Pt], branches: usize, angle: f64, ecart: f64) -> Vec<Prim> {
    let n = branches.max(1);
    let mut prims = Vec::new();
    for i in 0..n {
        let a0 = angle + 2.0 * PI * i as f64 / n as f64;
        let a1 = angle + 2.0 * PI * (i + 1) as f64 / n as f64;
        // Secteur entre le bras i (côté gauche décalé de ecart/2) et le bras i+1 (côté droit).
        let decoupe = |poly: &[Pt]| {
            let mut p: Vec<Pt> = poly.iter().map(|q| pt(q.x - c.x, q.y - c.y)).collect();
            // Au-dessus du bras a0 : -sin(a0)·x + cos(a0)·y ≥ ecart/2  ⇔  sin(a0)·x − cos(a0)·y ≤ −ecart/2
            p = couper_demi_plan(&p, a0.sin(), -a0.cos(), -ecart / 2.0);
            // En dessous du bras a1 : -sin(a1)·x + cos(a1)·y ≤ −ecart/2
            p = couper_demi_plan(&p, -a1.sin(), a1.cos(), -ecart / 2.0);
            if a1 - a0 > PI {
                // Secteur de plus d'un demi-tour (un seul bras) : les deux demi-plans ne suffisent pas, on garde tout
                // sauf la bande du bras.
                p = poly.iter().map(|q| pt(q.x - c.x, q.y - c.y)).collect();
            }
            p.into_iter().map(|q| pt(q.x + c.x, q.y + c.y)).collect::<Vec<Pt>>()
        };
        let ext = decoupe(exterieur);
        if ext.len() < 3 {
            continue;
        }
        let int = decoupe(interieur);
        let mut anneaux = vec![ext];
        if int.len() >= 3 {
            anneaux.push(int);
        }
        prims.push(Prim::Poly { anneaux });
    }
    prims
}

/// Un élément dessiné : primitive, polarité (sombre = ajoute, claire = efface) et objet auquel il appartient.
#[derive(Clone, Debug)]
pub struct Element {
    pub prim: Prim,
    pub sombre: bool,
    pub objet: u32,
}

/// Informations affichées pour un objet cliqué : paires (libellé, valeur).
pub type Info = Vec<(String, String)>;

/// Suite d'éléments dans l'ordre du fichier, avec la table des objets.
#[derive(Default, Clone)]
pub struct Dessin {
    pub elements: Vec<Element>,
    /// Informations de chaque objet (index = numéro d'objet).
    pub objets: Vec<u32>,
    pub infos: Vec<Info>,
    index_infos: HashMap<Info, u32>,
}

impl Dessin {
    /// Commence un nouvel objet ; renvoie son numéro.
    pub fn nouvel_objet(&mut self, info: Info) -> u32 {
        let id = match self.index_infos.get(&info) {
            Some(&i) => i,
            None => {
                let i = self.infos.len() as u32;
                self.index_infos.insert(info.clone(), i);
                self.infos.push(info);
                i
            }
        };
        self.objets.push(id);
        (self.objets.len() - 1) as u32
    }
    pub fn ajoute(&mut self, prim: Prim, sombre: bool, objet: u32) {
        self.elements.push(Element { prim, sombre, objet });
    }
    pub fn info_objet(&self, objet: u32) -> Option<&Info> {
        self.objets.get(objet as usize).and_then(|&i| self.infos.get(i as usize))
    }
    /// Boîte des axes des tracés (sans leur épaisseur) : les dimensions d'une carte se mesurent sur l'axe du contour.
    pub fn boite_axes(&self) -> BBox {
        let mut b = BBox::vide();
        for e in &self.elements {
            match &e.prim {
                Prim::Cap { a, b: q, .. } => {
                    b.ajoute(*a);
                    b.ajoute(*q);
                }
                autre => b.union(&autre.boite_englobante()),
            }
        }
        b
    }
    pub fn boite(&self) -> BBox {
        let mut b = BBox::vide();
        for e in &self.elements {
            if e.sombre {
                b.union(&e.prim.boite_englobante());
            }
        }
        b
    }
}

/// Géométrie d'une couche pour WebGL : suite de lots de même polarité. Chaque lot contient des segments à bouts ronds
/// (ax, ay, bx, by, r), des rectangles (cx, cy, hw, hh, angle, rayon) et des triangles (x, y par sommet), en f32.
pub struct Lot {
    pub sombre: bool,
    pub caps: Vec<f32>,
    pub boites: Vec<f32>,
    pub tris: Vec<f32>,
}

pub fn lots(dessin: &Dessin) -> Vec<Lot> {
    let mut lots: Vec<Lot> = Vec::new();
    for e in &dessin.elements {
        if lots.last().map(|l| l.sombre != e.sombre).unwrap_or(true) {
            lots.push(Lot { sombre: e.sombre, caps: Vec::new(), boites: Vec::new(), tris: Vec::new() });
        }
        let lot = lots.last_mut().unwrap();
        match &e.prim {
            Prim::Cap { a, b, r } => lot.caps.extend_from_slice(&[a.x as f32, a.y as f32, b.x as f32, b.y as f32, *r as f32]),
            Prim::Boite { c, hw, hh, ang, rad } => {
                lot.boites.extend_from_slice(&[c.x as f32, c.y as f32, *hw as f32, *hh as f32, *ang as f32, *rad as f32])
            }
            Prim::Poly { anneaux } => trianguler(anneaux, &mut lot.tris),
        }
    }
    lots
}

/// Triangule un polygone (contour + trous) avec earcut et ajoute les triangles à `sortie`.
pub fn trianguler(anneaux: &[Vec<Pt>], sortie: &mut Vec<f32>) {
    let mut coords: Vec<f64> = Vec::new();
    let mut trous: Vec<usize> = Vec::new();
    for (i, a) in anneaux.iter().enumerate() {
        let a = nettoyer(a);
        if a.len() < 3 {
            if i == 0 {
                return;
            }
            continue;
        }
        if i > 0 {
            trous.push(coords.len() / 2);
        }
        for p in &a {
            coords.push(p.x);
            coords.push(p.y);
        }
    }
    if coords.len() < 6 {
        return;
    }
    if let Ok(indices) = earcutr::earcut(&coords, &trous, 2) {
        for i in indices {
            sortie.push(coords[2 * i] as f32);
            sortie.push(coords[2 * i + 1] as f32);
        }
    }
}

/// Retire les points répétés et le point final qui referme l'anneau.
fn nettoyer(a: &[Pt]) -> Vec<Pt> {
    let mut r: Vec<Pt> = Vec::with_capacity(a.len());
    for p in a {
        if r.last().map(|q: &Pt| q.dist(*p) > 1e-9).unwrap_or(true) {
            r.push(*p);
        }
    }
    while r.len() > 1 && r[0].dist(*r.last().unwrap()) <= 1e-9 {
        r.pop();
    }
    r
}

/// Index spatial (grille) des éléments d'un dessin, pour trouver l'objet sous le pointeur.
pub struct Index {
    boite: BBox,
    pas: f64,
    nx: usize,
    ny: usize,
    cases: Vec<Vec<u32>>,
}

impl Index {
    pub fn nouveau(dessin: &Dessin) -> Index {
        let boite = {
            let mut b = BBox::vide();
            for e in &dessin.elements {
                b.union(&e.prim.boite_englobante());
            }
            if b.est_vide() { BBox { x0: 0.0, y0: 0.0, x1: 1.0, y1: 1.0 } } else { b }
        };
        let n = dessin.elements.len().max(1) as f64;
        let surface = ((boite.x1 - boite.x0) * (boite.y1 - boite.y0)).max(1e-6);
        let pas = (surface / n * 4.0).sqrt().max(0.05);
        let nx = (((boite.x1 - boite.x0) / pas).ceil() as usize).clamp(1, 2048);
        let ny = (((boite.y1 - boite.y0) / pas).ceil() as usize).clamp(1, 2048);
        let mut index = Index { boite, pas, nx, ny, cases: vec![Vec::new(); nx * ny] };
        for (i, e) in dessin.elements.iter().enumerate() {
            let b = e.prim.boite_englobante();
            let (i0, j0) = index.case(pt(b.x0, b.y0));
            let (i1, j1) = index.case(pt(b.x1, b.y1));
            for j in j0..=j1 {
                for k in i0..=i1 {
                    index.cases[j * nx + k].push(i as u32);
                }
            }
        }
        index
    }
    fn case(&self, p: Pt) -> (usize, usize) {
        let i = (((p.x - self.boite.x0) / self.pas).floor().max(0.0) as usize).min(self.nx - 1);
        let j = (((p.y - self.boite.y0) / self.pas).floor().max(0.0) as usize).min(self.ny - 1);
        (i, j)
    }
    /// L'objet sombre visible sous le point (le dernier dessiné, s'il n'est pas effacé ensuite).
    pub fn objet_sous(&self, dessin: &Dessin, p: Pt, tol: f64) -> Option<u32> {
        if !self.boite.contient(p, tol) {
            return None;
        }
        let mut candidats: Vec<u32> = Vec::new();
        let (i0, j0) = self.case(pt(p.x - tol, p.y - tol));
        let (i1, j1) = self.case(pt(p.x + tol, p.y + tol));
        for j in j0..=j1 {
            for k in i0..=i1 {
                candidats.extend_from_slice(&self.cases[j * self.nx + k]);
            }
        }
        candidats.sort_unstable();
        candidats.dedup();
        for &i in candidats.iter().rev() {
            let e = &dessin.elements[i as usize];
            if e.prim.boite_englobante().contient(p, tol) && e.prim.contient(p, tol) {
                return if e.sombre { Some(e.objet) } else { None };
            }
        }
        None
    }
}

/// Contours d'une primitive (pour les opérations booléennes).
pub fn contours(p: &Prim) -> Vec<Vec<[f64; 2]>> {
    let en = |v: &[Pt]| v.iter().map(|q| [q.x, q.y]).collect::<Vec<_>>();
    match p {
        Prim::Cap { a, b, r } => {
            let r = r.max(1e-6);
            if a.dist(*b) < 1e-12 {
                return vec![en(&cercle(*a, r))];
            }
            // Capsule : demi-cercle autour de b puis demi-cercle autour de a.
            let ang = (b.y - a.y).atan2(b.x - a.x);
            let n = segments_cercle(r, PI);
            let mut pts = Vec::with_capacity(2 * n + 2);
            for i in 0..=n {
                let t = ang - PI / 2.0 + PI * i as f64 / n as f64;
                pts.push(pt(b.x + r * t.cos(), b.y + r * t.sin()));
            }
            for i in 0..=n {
                let t = ang + PI / 2.0 + PI * i as f64 / n as f64;
                pts.push(pt(a.x + r * t.cos(), a.y + r * t.sin()));
            }
            vec![en(&pts)]
        }
        Prim::Boite { c, hw, hh, ang, rad } => {
            let base = rectangle_coins(pt(0.0, 0.0), 2.0 * hw, 2.0 * hh, *rad, false, if *rad > 0.0 { 0b1111 } else { 0 });
            let t = Transfo::translation(c.x, c.y).apres(&Transfo::rotation(*ang));
            vec![base.iter().map(|q| {
                let r = t.applique(*q);
                [r.x, r.y]
            }).collect()]
        }
        Prim::Poly { anneaux } => anneaux.iter().map(|a| en(a)).collect(),
    }
}

/// Forme résultant d'une suite de primitives exposées (ajoutées) ou non (retirées), comme une macro d'ouverture dont
/// les parties « exposition 0 » font des trous transparents. Renvoie des polygones avec leurs trous.
pub fn forme_booleenne(prims: &[(Prim, bool)]) -> Vec<Prim> {
    use i_overlay::core::fill_rule::FillRule;
    use i_overlay::core::overlay_rule::OverlayRule;
    use i_overlay::float::single::SingleFloatOverlay;
    let mut acc: Vec<Vec<Vec<[f64; 2]>>> = Vec::new();
    for (p, expose) in prims {
        let c = contours(p);
        if c.is_empty() {
            continue;
        }
        let operande: Vec<Vec<Vec<[f64; 2]>>> = vec![c];
        if *expose {
            acc = if acc.is_empty() { operande.overlay(&Vec::<Vec<Vec<[f64; 2]>>>::new(), OverlayRule::Subject, FillRule::EvenOdd) } else { acc.overlay(&operande, OverlayRule::Union, FillRule::EvenOdd) };
        } else if !acc.is_empty() {
            acc = acc.overlay(&operande, OverlayRule::Difference, FillRule::EvenOdd);
        }
    }
    acc.into_iter()
        .map(|forme| Prim::Poly { anneaux: forme.into_iter().map(|c| c.into_iter().map(|q| pt(q[0], q[1])).collect()).collect() })
        .collect()
}

/// Anneaux fermés d'un contour de carte (traits du contour, quel que soit leur ordre ou leur sens) : les segments sont
/// enchaînés bout à bout, à 10 µm près. Les morceaux qui ne se referment pas sont ignorés.
pub fn anneaux_contour(d: &Dessin) -> Vec<Vec<Pt>> {
    let mut segments: Vec<(Pt, Pt)> = Vec::new();
    for e in &d.elements {
        match &e.prim {
            Prim::Cap { a, b, .. } if a.dist(*b) > 1e-6 => segments.push((*a, *b)),
            Prim::Poly { anneaux } => {
                for an in anneaux {
                    for i in 0..an.len() {
                        let (p, q) = (an[i], an[(i + 1) % an.len()]);
                        if p.dist(q) > 1e-6 {
                            segments.push((p, q));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let tol = 0.01;
    let cle = |p: Pt| ((p.x / tol).round() as i64, (p.y / tol).round() as i64);
    let mut extremites: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, (a, b)) in segments.iter().enumerate() {
        extremites.entry(cle(*a)).or_default().push(i);
        extremites.entry(cle(*b)).or_default().push(i);
    }
    let voisin = |p: Pt, utilise: &[bool], extremites: &HashMap<(i64, i64), Vec<usize>>| -> Option<usize> {
        let (cx, cy) = cle(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(v) = extremites.get(&(cx + dx, cy + dy)) {
                    for &i in v {
                        if !utilise[i] {
                            let (a, b) = segments[i];
                            if a.dist(p) <= tol * 1.5 || b.dist(p) <= tol * 1.5 {
                                return Some(i);
                            }
                        }
                    }
                }
            }
        }
        None
    };
    let mut utilise = vec![false; segments.len()];
    let mut anneaux = Vec::new();
    for depart in 0..segments.len() {
        if utilise[depart] {
            continue;
        }
        utilise[depart] = true;
        let (a, b) = segments[depart];
        let mut anneau = vec![a, b];
        let mut fin = b;
        while let Some(i) = voisin(fin, &utilise, &extremites) {
            utilise[i] = true;
            let (p, q) = segments[i];
            fin = if p.dist(fin) <= q.dist(fin) { q } else { p };
            anneau.push(fin);
            if fin.dist(a) <= tol * 1.5 {
                break;
            }
        }
        if anneau.len() >= 4 && fin.dist(a) <= tol * 1.5 {
            anneau.pop();
            anneaux.push(anneau);
        }
    }
    anneaux
}

pub fn aire(a: &[Pt]) -> f64 {
    let mut s = 0.0;
    for i in 0..a.len() {
        let (p, q) = (a[i], a[(i + 1) % a.len()]);
        s += p.x * q.y - q.x * p.y;
    }
    s / 2.0
}
