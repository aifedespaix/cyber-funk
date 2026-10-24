//! Les captures de `car_demo` et leurs mesures : luminance moyenne et comptages de pixels dans un rectangle.

/// Une capture : RGBA sRGB 8 bits, ligne par ligne depuis le coin haut gauche.
pub(crate) struct Shot {
    pub(crate) width: u32,
    pub(crate) rgba: Vec<u8>,
}

/// Un rectangle `[x0, x1) × [y0, y1)`.
pub(crate) type Rect = ((u32, u32), (u32, u32));

impl Shot {
    fn pixel(&self, x: u32, y: u32) -> [f32; 3] {
        let i = 4 * (y as usize * self.width as usize + x as usize);
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]].map(f32::from)
    }

    pub(crate) fn height(&self) -> u32 {
        (self.rgba.len() / 4 / self.width as usize) as u32
    }

    /// Luminance moyenne d'un rectangle.
    pub(crate) fn mean_luma(&self, ((x0, y0), (x1, y1)): Rect) -> f32 {
        let mut sum = 0.0;
        for y in y0..y1 {
            for x in x0..x1 {
                sum += luma(self.pixel(x, y));
            }
        }
        sum / ((x1 - x0) * (y1 - y0)) as f32
    }

    /// Nombre de pixels du rectangle qui vérifient `test`.
    pub(crate) fn count_in(&self, ((x0, y0), (x1, y1)): Rect, test: impl Fn([f32; 3]) -> bool) -> usize {
        (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| test(self.pixel(x, y)))
            .count()
    }
}

pub(crate) fn luma(p: [f32; 3]) -> f32 {
    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
}
