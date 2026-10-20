//! Les captures de `road_demo` et leurs mesures : luminance, comptages de pixels, écarts entre deux images.

/// Une capture : RGBA sRGB 8 bits, ligne par ligne depuis le coin haut gauche.
pub(crate) struct Shot {
    pub(crate) width: u32,
    pub(crate) rgba: Vec<u8>,
}

impl Shot {
    pub(crate) fn pixel(&self, x: u32, y: u32) -> [f32; 3] {
        let i = 4 * (y as usize * self.width as usize + x as usize);
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]].map(f32::from)
    }

    pub(crate) fn height(&self) -> u32 {
        (self.rgba.len() / 4 / self.width as usize) as u32
    }

    /// Luminance moyenne d'un rectangle `[x0, x1) × [y0, y1)`.
    pub(crate) fn mean_luma(&self, (x0, y0): (u32, u32), (x1, y1): (u32, u32)) -> f32 {
        let mut sum = 0.0;
        for y in y0..y1 {
            for x in x0..x1 {
                sum += luma(self.pixel(x, y));
            }
        }
        sum / ((x1 - x0) * (y1 - y0)) as f32
    }

    /// Nombre de pixels qui vérifient `test`.
    pub(crate) fn count(&self, test: impl Fn([f32; 3]) -> bool) -> usize {
        self.count_in(((0, 0), (self.width, self.height())), test)
    }

    /// Nombre de pixels du rectangle `[x0, x1) × [y0, y1)` qui vérifient `test`.
    pub(crate) fn count_in(
        &self,
        ((x0, y0), (x1, y1)): ((u32, u32), (u32, u32)),
        test: impl Fn([f32; 3]) -> bool,
    ) -> usize {
        (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| test(self.pixel(x, y)))
            .count()
    }
}

pub(crate) fn luma(p: [f32; 3]) -> f32 {
    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
}

/// Écart maximal, par canal, entre deux captures.
pub(crate) fn worst_diff(a: &Shot, b: &Shot) -> f32 {
    let mut worst = 0.0_f32;
    for y in 0..a.height().min(b.height()) {
        for x in 0..a.width.min(b.width) {
            let (p, q) = (a.pixel(x, y), b.pixel(x, y));
            worst = worst.max((0..3).map(|c| (p[c] - q[c]).abs()).fold(0.0, f32::max));
        }
    }
    worst
}

/// Part des pixels qui diffèrent de plus de `level` (sur 255) sur un canal.
pub(crate) fn differing_share(a: &Shot, b: &Shot, level: f32) -> f32 {
    let total = (a.width * a.height()) as f32;
    let n = a.count_pairs(b, |p, q| (0..3).any(|c| (p[c] - q[c]).abs() > level));
    n as f32 / total
}

impl Shot {
    pub(crate) fn count_pairs(&self, other: &Shot, test: impl Fn([f32; 3], [f32; 3]) -> bool) -> usize {
        let mut n = 0;
        for y in 0..self.height().min(other.height()) {
            for x in 0..self.width.min(other.width) {
                if test(self.pixel(x, y), other.pixel(x, y)) {
                    n += 1;
                }
            }
        }
        n
    }
}
