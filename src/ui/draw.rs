//! Formas e cores para desenhar a interface dos clientes com tiny-skia.

use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point, Shader,
    SpreadMode, Transform,
};

thread_local! {
    /// Escala da superfície sendo desenhada (pixels por ponto): 1 nas telas comuns,
    /// 1,25/1,5/2 em telas com escala. As funções daqui recebem coordenadas em pontos
    /// e desenham na imagem em pixels (nítido, sem o compositor esticar). Só os apps
    /// do mide-ui mudam a escala; o resto desenha em 1.
    static SCALE: std::cell::Cell<f32> = const { std::cell::Cell::new(1.0) };
}

/// Muda a escala dos próximos desenhos nesta thread.
pub fn set_scale(scale: f32) {
    SCALE.with(|s| s.set(if scale.is_finite() && scale > 0.0 { scale } else { 1.0 }));
}

pub fn scale() -> f32 {
    SCALE.with(|s| s.get())
}

/// Transformação de pontos para pixels (para `fill_path`, `stroke_path`...).
pub fn tf() -> Transform {
    let s = scale();
    Transform::from_scale(s, s)
}

/// Uma coordenada em pontos como pixel inteiro.
pub fn px(v: f32) -> i32 {
    (v * scale()).round() as i32
}

/// Retângulo arredondado como caminho, com cantos em curvas cúbicas.
pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    const K: f32 = 0.5523;
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + r * K, y, x + w, y + r - r * K, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + r * K, x + w - r + r * K, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - r * K, y + h, x, y + h - r + r * K, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - r * K, x + r - r * K, y, x + r, y);
    pb.close();
    pb.finish()
}

/// Retângulo com os cantos de cima arredondados em `top` e os de baixo em `bottom`
/// (seleção contínua de várias linhas de uma lista: um bloco só).
pub fn rounded_rect_tb(x: f32, y: f32, w: f32, h: f32, top: f32, bottom: f32) -> Option<tiny_skia::Path> {
    const K: f32 = 0.5523;
    let t = top.min(w / 2.0).min(h / 2.0).max(0.0);
    let b = bottom.min(w / 2.0).min(h / 2.0).max(0.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + t, y);
    pb.line_to(x + w - t, y);
    pb.cubic_to(x + w - t + t * K, y, x + w, y + t - t * K, x + w, y + t);
    pb.line_to(x + w, y + h - b);
    pb.cubic_to(x + w, y + h - b + b * K, x + w - b + b * K, y + h, x + w - b, y + h);
    pb.line_to(x + b, y + h);
    pb.cubic_to(x + b - b * K, y + h, x, y + h - b + b * K, x, y + h - b);
    pb.line_to(x, y + t);
    pb.cubic_to(x, y + t - t * K, x + t - t * K, y, x + t, y);
    pb.close();
    pb.finish()
}

/// Preenche um retângulo com raios diferentes em cima e embaixo ([`rounded_rect_tb`]).
#[allow(clippy::too_many_arguments)]
pub fn fill_rounded_tb(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, top: f32, bottom: f32, color: Color) {
    if let Some(path) = rounded_rect_tb(x, y, w, h, top, bottom) {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, FillRule::Winding, tf(), None);
    }
}

/// Preenche um retângulo arredondado com uma cor (mistura normal, borda suave).
///
/// Desenhado direto nos pixels pela distância à borda (como `soft_shadow`): é a forma
/// mais desenhada da interface (botões, cápsulas, linhas escolhidas, painéis), e o
/// caminho genérico do tiny-skia (amostra cada pixel 4 vezes na borda) custava uma boa
/// parte de cada quadro numa janela cheia de controles.
pub fn fill_rounded(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    let c = color.premultiply().to_color_u8();
    let (sr, sg, sb, sa) = (c.red() as u32, c.green() as u32, c.blue() as u32, c.alpha() as u32);
    if sa == 0 {
        return;
    }
    let k = scale();
    let shape = RoundedPx::new(x * k, y * k, w * k, h * k, r * k);
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    let opaque = c;
    let pixels = pixmap.pixels_mut();
    // Os pixels como bytes, para misturar (mesma memória: RGBA premultiplicado).
    shape.cover(width, height, |start, len, cov| {
        let cc = (cov * 255.0 + 0.5) as u32;
        let (r, g, b, a) = if cc >= 255 { (sr, sg, sb, sa) } else { (div255(sr * cc), div255(sg * cc), div255(sb * cc), div255(sa * cc)) };
        if a == 255 {
            pixels[start..start + len].fill(opaque);
            return;
        }
        let span = &mut pixels[start..start + len];
        let inv = 255 - a;
        for px in span.iter_mut() {
            // Com o arredondamento, uma cor pode passar do alfa por 1: fica no alfa.
            let oa = (a + div255(px.alpha() as u32 * inv)).min(255);
            let ch = |s: u32, d: u8| (s + div255(d as u32 * inv)).min(oa) as u8;
            if let Some(m) = tiny_skia::PremultipliedColorU8::from_rgba(ch(r, px.red()), ch(g, px.green()), ch(b, px.blue()), oa as u8) {
                *px = m;
            }
        }
    });
}

/// Apaga (deixa transparente) um retângulo de cantos arredondados: tira a sombra de
/// baixo de um painel translúcido antes de pintá-lo (senão o vidro fica acinzentado).
pub fn clear_rounded(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32) {
    let k = scale();
    let shape = RoundedPx::new(x * k, y * k, w * k, h * k, r * k);
    let width = pixmap.width() as usize;
    let data = pixmap.data_mut();
    shape.cover(width, data.len() / 4 / width.max(1), |start, len, cov| {
        let keep = 255 - (cov * 255.0 + 0.5) as u32;
        let span = &mut data[start * 4..(start + len) * 4];
        if keep == 0 {
            span.fill(0);
            return;
        }
        for v in span.iter_mut() {
            *v = div255(*v as u32 * keep) as u8;
        }
    });
}

/// Contorno de um retângulo arredondado, com a linha de largura `width` centrada na
/// borda (como `stroke_path` com `rounded_rect`), desenhado direto nos pixels: a faixa
/// entre a forma aumentada e a diminuída de meia largura.
#[allow(clippy::too_many_arguments)]
pub fn stroke_rounded(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, width: f32, color: Color) {
    let c = color.premultiply().to_color_u8();
    let (sr, sg, sb, sa) = (c.red() as u32, c.green() as u32, c.blue() as u32, c.alpha() as u32);
    if sa == 0 || width <= 0.0 {
        return;
    }
    let k = scale();
    let half = width / 2.0;
    let outer = RoundedPx::new((x - half) * k, (y - half) * k, (w + width) * k, (h + width) * k, (r + half).max(0.0) * k);
    let inner = RoundedPx::new((x + half) * k, (y + half) * k, (w - width).max(0.0) * k, (h - width).max(0.0) * k, (r - half).max(0.0) * k);
    let pw = pixmap.width() as usize;
    let ph = pixmap.height() as usize;
    let data = pixmap.data_mut();
    let (x0, x1) = ((outer.x.floor() as i32).max(0), ((outer.x + outer.w).ceil() as i32).min(pw as i32));
    let (y0, y1) = ((outer.y.floor() as i32).max(0), ((outer.y + outer.h).ceil() as i32).min(ph as i32));
    let mut put = |idx: usize, cov: f32| {
        let cc = (cov * 255.0 + 0.5) as u32;
        if cc == 0 {
            return;
        }
        let (r, g, b, a) = (div255(sr * cc), div255(sg * cc), div255(sb * cc), div255(sa * cc));
        let inv = 255 - a;
        let px = &mut data[idx * 4..idx * 4 + 4];
        px[0] = (r + div255(px[0] as u32 * inv)) as u8;
        px[1] = (g + div255(px[1] as u32 * inv)) as u8;
        px[2] = (b + div255(px[2] as u32 * inv)) as u8;
        px[3] = (a + div255(px[3] as u32 * inv)) as u8;
    };
    for row in y0..y1 {
        let py = row as f32 + 0.5;
        let ring = |col: i32| {
            let px = col as f32 + 0.5;
            (outer.coverage(px, py) - inner.coverage(px, py)).clamp(0.0, 1.0)
        };
        // Nas linhas do meio só as duas bordas têm linha: anda da esquerda e da direita
        // até o anel acabar (o miolo é pulado).
        let base = row as usize * pw;
        let mid_inner = inner.coverage(inner.x + inner.w / 2.0, py);
        if mid_inner >= 1.0 {
            let mut col = x0;
            while col < x1 {
                let px = col as f32 + 0.5;
                let c = ring(col);
                put(base + col as usize, c);
                if inner.coverage(px, py) >= 1.0 {
                    break;
                }
                col += 1;
            }
            let mut col = x1 - 1;
            while col >= x0 {
                let px = col as f32 + 0.5;
                let c = ring(col);
                put(base + col as usize, c);
                if inner.coverage(px, py) >= 1.0 {
                    break;
                }
                col -= 1;
            }
        } else {
            for col in x0..x1 {
                put(base + col as usize, ring(col));
            }
        }
    }
}

/// Divisão por 255 arredondada, em inteiros (a mistura das cores premultiplicadas).
#[inline]
fn div255(v: u32) -> u32 {
    (v + 128 + ((v + 128) >> 8)) >> 8
}

/// Retângulo arredondado em pixels, com a cobertura de cada pixel pela distância do
/// centro dele à borda (borda suave de 1 pixel).
struct RoundedPx {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
}

impl RoundedPx {
    fn new(x: f32, y: f32, w: f32, h: f32, r: f32) -> Self {
        let (w, h) = (w.max(0.0), h.max(0.0));
        Self { x, y, w, h, r: r.min(w / 2.0).min(h / 2.0).max(0.0) }
    }

    /// Cobertura (0 a 1) do pixel com centro em (`px`, `py`).
    #[inline]
    fn coverage(&self, px: f32, py: f32) -> f32 {
        if self.w <= 0.0 || self.h <= 0.0 {
            return 0.0;
        }
        let (hw, hh) = (self.w / 2.0, self.h / 2.0);
        let (qx, qy) = ((px - self.x - hw).abs() - hw + self.r, (py - self.y - hh).abs() - hh + self.r);
        let d = if qx > 0.0 && qy > 0.0 { qx.hypot(qy) - self.r } else { qx.max(qy) - self.r };
        (0.5 - d).clamp(0.0, 1.0)
    }

    /// Chama `put(primeiro_pixel, quantos, cobertura)` para as partes cobertas, linha a
    /// linha: as bordas pixel a pixel e o miolo de cada linha num trecho só.
    fn cover(&self, width: usize, height: usize, mut put: impl FnMut(usize, usize, f32)) {
        if self.w <= 0.0 || self.h <= 0.0 {
            return;
        }
        let x0 = (self.x.floor() as i32).max(0);
        let x1 = ((self.x + self.w).ceil() as i32).min(width as i32);
        let y0 = (self.y.floor() as i32).max(0);
        let y1 = ((self.y + self.h).ceil() as i32).min(height as i32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let mid = self.x + self.w / 2.0;
        for row in y0..y1 {
            let py = row as f32 + 0.5;
            let full = self.coverage(mid, py);
            if full <= 0.0 {
                continue;
            }
            let base = row as usize * width;
            // Da esquerda até chegar à cobertura do meio da linha.
            let mut left = x0;
            while left < x1 {
                let c = self.coverage(left as f32 + 0.5, py);
                if c >= full - 1e-4 {
                    break;
                }
                if c > 0.0 {
                    put(base + left as usize, 1, c);
                }
                left += 1;
            }
            // Da direita, sem passar da esquerda.
            let mut right = x1 - 1;
            while right >= left {
                let c = self.coverage(right as f32 + 0.5, py);
                if c >= full - 1e-4 {
                    break;
                }
                if c > 0.0 {
                    put(base + right as usize, 1, c);
                }
                right -= 1;
            }
            if right >= left {
                put(base + left as usize, (right - left + 1) as usize, full);
            }
        }
    }
}

/// Pinta um retângulo de cantos arredondados com uma cor sólida, *substituindo* o que
/// havia por baixo (como `clear_rounded` seguido de `fill_rounded`, bem mais rápido em
/// áreas grandes): cada linha é escrita direto, com a extensão calculada pelo canto, e
/// só os pixels da borda misturam (antisserrilhado). `rgba` não premultiplicado.
pub fn paint_rounded_solid(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, rgba: [u8; 4]) {
    let k = scale();
    let (x, y, w, h, r) = (x * k, y * k, w * k, h * k, r * k);
    let (pw, ph) = (pixmap.width() as i32, pixmap.height() as i32);
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let k = rgba[3] as f32 / 255.0;
    let solid = |cov: f32| {
        let a = k * cov;
        tiny_skia::PremultipliedColorU8::from_rgba(
            (rgba[0] as f32 * a).round() as u8,
            (rgba[1] as f32 * a).round() as u8,
            (rgba[2] as f32 * a).round() as u8,
            (255.0 * a).round() as u8,
        )
        .unwrap()
    };
    let full = solid(1.0);
    let pixels = pixmap.pixels_mut();
    let y0 = (y.floor() as i32).max(0);
    let y1 = ((y + h).ceil() as i32).min(ph);
    for row in y0..y1 {
        // Cobertura vertical da linha (bordas de cima e de baixo fracionárias).
        let (top, bottom) = (row as f32, row as f32 + 1.0);
        let vcov = (bottom.min(y + h) - top.max(y)).clamp(0.0, 1.0);
        if vcov <= 0.0 {
            continue;
        }
        // Meia-largura recolhida pelo canto no centro da linha.
        let cy = top + 0.5;
        let dy = if cy < y + r { y + r - cy } else if cy > y + h - r { cy - (y + h - r) } else { 0.0 };
        let inset = if dy > 0.0 { r - (r * r - dy.min(r) * dy.min(r)).sqrt() } else { 0.0 };
        let (left, right) = (x + inset, x + w - inset);
        let xa = (left.floor() as i32).max(0);
        let xb = (right.ceil() as i32).min(pw);
        let base = (row * pw) as usize;
        for col in xa..xb {
            let (cl, cr) = (col as f32, col as f32 + 1.0);
            let hcov = (cr.min(right) - cl.max(left)).clamp(0.0, 1.0);
            let cov = hcov * vcov;
            let p = &mut pixels[base + col as usize];
            if cov >= 0.999 {
                *p = full;
            } else if cov > 0.0 {
                // Borda: substitui na proporção da cobertura (o que havia fica com o
                // resto), como apagar e pintar com antisserrilhado.
                let c = solid(cov);
                let inv = ((1.0 - cov) * 255.0).round() as u32;
                let mix = |s: u8, d: u8| (s as u32 + (d as u32 * inv + 127) / 255).min(255) as u8;
                let a = mix(c.alpha(), p.alpha());
                if let Some(out) = tiny_skia::PremultipliedColorU8::from_rgba(mix(c.red(), p.red()).min(a), mix(c.green(), p.green()).min(a), mix(c.blue(), p.blue()).min(a), a) {
                    *p = out;
                }
            }
        }
    }
}

/// Sombra suave em volta de um retângulo arredondado, calculada direto nos pixels
/// pela distância até a borda: só a faixa em volta é percorrida (o miolo, que o painel
/// cobre, é pulado), então é barata mesmo em painéis grandes. `spread` é a largura da
/// sombra, `drop` desce a sombra e `alpha` é a força perto da borda.
pub fn soft_shadow(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, spread: f32, drop: f32, alpha: u8) {
    let k = scale();
    let (x, y, w, h, r, spread, drop) = (x * k, y * k, w * k, h * k, r * k, spread * k, drop * k);
    let (pw, ph) = (pixmap.width() as i32, pixmap.height() as i32);
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let (hw, hh) = (w / 2.0, h / 2.0);
    let (cx, cy) = (x + hw, y + hh + drop);
    let sdf = |px: f32, py: f32| {
        let (qx, qy) = ((px - cx).abs() - hw + r, (py - cy).abs() - hh + r);
        // Fora dos cantos a distância é reta (sem raiz quadrada).
        if qx <= 0.0 || qy <= 0.0 {
            return qx.max(qy) - r;
        }
        qx.hypot(qy) - r
    };
    let x0 = ((x - spread).floor() as i32).max(0);
    let x1 = ((x + w + spread).ceil() as i32).min(pw);
    let y0 = ((y - spread + drop).floor() as i32).max(0);
    let y1 = ((y + h + spread + drop).ceil() as i32).min(ph);
    // O que o painel cobre (pulado): a faixa do meio na largura toda das linhas do
    // painel, e as colunas longe dos cantos nas linhas de cima e de baixo dele.
    let (in_x0, in_x1) = ((x + 1.0).ceil() as i32, (x + w - 1.0).floor() as i32);
    let (mid_y0, mid_y1) = ((y + r).ceil() as i32, (y + h - r).floor() as i32);
    let (cap_x0, cap_x1) = ((x + r).ceil() as i32, (x + w - r).floor() as i32);
    let (cap_y0, cap_y1) = ((y + 1.0).ceil() as i32, (y + h - 1.0).floor() as i32);
    let a = alpha as f32;
    let pixels = pixmap.pixels_mut();
    for row in y0..y1 {
        let mut col = x0;
        while col < x1 {
            if row >= mid_y0 && row < mid_y1 && col >= in_x0 && col < in_x1 {
                col = in_x1;
                continue;
            }
            if row >= cap_y0 && row < cap_y1 && col >= cap_x0 && col < cap_x1 {
                col = cap_x1;
                continue;
            }
            let d = sdf(col as f32 + 0.5, row as f32 + 0.5);
            let t = (1.0 - d.max(0.0) / spread).clamp(0.0, 1.0);
            let sa = (a * t * t).round() as u32;
            if sa > 0 {
                let p = &mut pixels[(row * pw + col) as usize];
                let inv = 255 - sa;
                let k = |c: u8| ((c as u32 * inv + 127) / 255) as u8;
                let out_a = (sa + (p.alpha() as u32 * inv + 127) / 255).min(255) as u8;
                if let Some(c) = tiny_skia::PremultipliedColorU8::from_rgba(k(p.red()).min(out_a), k(p.green()).min(out_a), k(p.blue()).min(out_a), out_a) {
                    *p = c;
                }
            }
            col += 1;
        }
    }
}

/// Gradiente vertical de `top` (em `y0`) até `bottom` (em `y1`).
pub fn vertical_gradient(y0: f32, y1: f32, top: Color, bottom: Color) -> Shader<'static> {
    LinearGradient::new(
        Point::from_xy(0.0, y0),
        Point::from_xy(0.0, y1),
        vec![GradientStop::new(0.0, top), GradientStop::new(1.0, bottom)],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap_or(Shader::SolidColor(top))
}

/// Vidro claro da dock do mide (e da seleção do launcher): preenchimento translúcido
/// um pouco mais claro em cima, borda fina mais escura em cima e mais clara embaixo
/// (dá volume) e um brilho interno de 1 px logo abaixo da borda de cima.
pub fn glass(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, radius: f32) {
    if let Some(path) = rounded_rect(x + 0.5, y + 0.5, w - 1.0, h - 1.0, radius) {
        let mut fill = Paint::default();
        fill.anti_alias = true;
        fill.shader = vertical_gradient(y, y + h, rgba(255, 255, 255, 92), rgba(235, 235, 245, 70));
        pixmap.fill_path(&path, &fill, FillRule::Winding, tf(), None);
        let mut border = Paint::default();
        border.anti_alias = true;
        border.shader = vertical_gradient(y, y + h, rgba(0, 0, 0, 70), rgba(255, 255, 255, 110));
        pixmap.stroke_path(&path, &border, &tiny_skia::Stroke { width: 1.0, ..Default::default() }, tf(), None);
    }
    if let Some(path) = rounded_rect(x + 1.5, y + 1.5, w - 3.0, h - 3.0, (radius - 1.0).max(0.0)) {
        let mut shine = Paint::default();
        shine.anti_alias = true;
        shine.shader = vertical_gradient(y, y + h, rgba(255, 255, 255, 60), rgba(255, 255, 255, 0));
        pixmap.stroke_path(&path, &shine, &tiny_skia::Stroke { width: 1.0, ..Default::default() }, tf(), None);
    }
}

/// Véu do vidro líquido: só um preenchimento claro e leve, sem bordas. A borda curva,
/// o reflexo e o tom são desenhados pelo compositor atrás da superfície.
pub fn glass_veil(pixmap: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, radius: f32) {
    if let Some(path) = rounded_rect(x, y, w, h, radius) {
        let mut fill = Paint::default();
        fill.anti_alias = true;
        fill.shader = vertical_gradient(y, y + h, rgba(255, 255, 255, 34), rgba(255, 255, 255, 18));
        pixmap.fill_path(&path, &fill, FillRule::Winding, tf(), None);
    }
}

/// Cola `src` (já em pixels) em `dst` na posição (`x`, `y`) em pontos, com mistura normal
/// (as duas imagens em RGBA premultiplicado) e `opacity` de 0 a 1. Faz o mesmo que
/// `draw_pixmap` nesse caso, bem mais rápido: pixel opaco é copiado, transparente é
/// pulado, e só as bordas misturam. Ícones e miniaturas passam por aqui.
pub fn blit(dst: &mut Pixmap, src: &Pixmap, x: i32, y: i32, opacity: f32) {
    blit_px(dst, src, px(x as f32), px(y as f32), opacity);
}

/// Arredonda os cantos da imagem com raio `r` (em pixels), deixando o lado de fora
/// transparente, com borda suave: a janela fora do mide (lá, o compositor arredonda).
pub fn round_corners(pixmap: &mut Pixmap, r: f32) {
    let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
    let ri = r.ceil() as i32;
    if ri <= 0 || w < 2 * ri || h < 2 * ri {
        return;
    }
    let data = pixmap.data_mut();
    for cy in 0..ri {
        for cx in 0..ri {
            // Distância do centro do pixel ao centro do arco; cobertura por 1 pixel de borda.
            let (dx, dy) = (r - (cx as f32 + 0.5), r - (cy as f32 + 0.5));
            let d = (dx * dx + dy * dy).sqrt();
            let cover = (r - d + 0.5).clamp(0.0, 1.0);
            if cover >= 1.0 {
                continue;
            }
            for (x, y) in [(cx, cy), (w - 1 - cx, cy), (cx, h - 1 - cy), (w - 1 - cx, h - 1 - cy)] {
                let i = ((y * w + x) * 4) as usize;
                for c in &mut data[i..i + 4] {
                    *c = (*c as f32 * cover).round() as u8;
                }
            }
        }
    }
}

/// Copia pixels RGBA opacos (um quadro de vídeo, `w`x`h`) na posição em pixels, linha
/// a linha, sem misturar transparência: o caminho rápido para o player.
pub fn copy_rgba(dst: &mut Pixmap, pixels: &[u8], w: u32, h: u32, x: i32, y: i32) {
    let (dw, dh) = (dst.width() as i32, dst.height() as i32);
    let (w, h) = (w as i32, h as i32);
    let (x0, x1) = (x.max(0), (x + w).min(dw));
    if x0 >= x1 || pixels.len() < (w * h * 4) as usize {
        return;
    }
    let data = dst.data_mut();
    for row in y.max(0)..(y + h).min(dh) {
        let s = (((row - y) * w + (x0 - x)) * 4) as usize;
        let d = ((row * dw + x0) * 4) as usize;
        let n = ((x1 - x0) * 4) as usize;
        data[d..d + n].copy_from_slice(&pixels[s..s + n]);
    }
}

/// Como `blit`, com a posição já em pixels (colar uma área desenhada à parte). Direto
/// nos bytes (RGBA premultiplicado nos dois lados): pixel opaco é copiado, transparente
/// é pulado, e o resto mistura em inteiros.
pub fn blit_px(dst: &mut Pixmap, src: &Pixmap, x: i32, y: i32, opacity: f32) {
    let (dw, dh, sw, sh) = (dst.width() as i32, dst.height() as i32, src.width() as i32, src.height() as i32);
    let (x0, y0) = (x.max(0), y.max(0));
    let (x1, y1) = ((x + sw).min(dw), (y + sh).min(dh));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let k = (opacity.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    if k == 0 {
        return;
    }
    let n = (x1 - x0) as usize * 4;
    let src_data = src.data();
    let dst_data = dst.data_mut();
    for row in y0..y1 {
        let s0 = (((row - y) * sw + (x0 - x)) * 4) as usize;
        let d0 = ((row * dw + x0) * 4) as usize;
        let srow = &src_data[s0..s0 + n];
        let drow = &mut dst_data[d0..d0 + n];
        if k >= 255 {
            for (sp, dp) in srow.chunks_exact(4).zip(drow.chunks_exact_mut(4)) {
                let a = sp[3] as u32;
                if a == 255 {
                    dp.copy_from_slice(sp);
                } else if a != 0 {
                    let inv = 255 - a;
                    dp[0] = (sp[0] as u32 + div255(dp[0] as u32 * inv)).min(255) as u8;
                    dp[1] = (sp[1] as u32 + div255(dp[1] as u32 * inv)).min(255) as u8;
                    dp[2] = (sp[2] as u32 + div255(dp[2] as u32 * inv)).min(255) as u8;
                    dp[3] = (a + div255(dp[3] as u32 * inv)).min(255) as u8;
                }
            }
        } else {
            for (sp, dp) in srow.chunks_exact(4).zip(drow.chunks_exact_mut(4)) {
                let a = div255(sp[3] as u32 * k);
                if a == 0 {
                    continue;
                }
                let inv = 255 - a;
                dp[0] = (div255(sp[0] as u32 * k) + div255(dp[0] as u32 * inv)).min(255) as u8;
                dp[1] = (div255(sp[1] as u32 * k) + div255(dp[1] as u32 * inv)).min(255) as u8;
                dp[2] = (div255(sp[2] as u32 * k) + div255(dp[2] as u32 * inv)).min(255) as u8;
                dp[3] = (a + div255(dp[3] as u32 * inv)).min(255) as u8;
            }
        }
    }
}

/// Imagem feita em pontos (miniaturas do app) colada em (`x`, `y`): em telas com
/// escala, ampliada para continuar do mesmo tamanho.
pub fn blit_image(dst: &mut Pixmap, src: &Pixmap, x: f32, y: f32) {
    let k = scale();
    if k == 1.0 {
        blit_px(dst, src, x.round() as i32, y.round() as i32, 1.0);
        return;
    }
    let paint = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
    dst.draw_pixmap(0, 0, src.as_ref(), &paint, Transform::from_row(k, 0.0, 0.0, k, (x * k).round(), (y * k).round()), None);
}

/// Como `blit`, mas pinta a imagem toda numa cor só (mantendo a transparência):
/// ícones "symbolic" na cor de destaque.
pub fn blit_tinted(dst: &mut Pixmap, src: &Pixmap, x: i32, y: i32, rgb: [u8; 3]) {
    let mut tinted = src.clone();
    for p in tinted.pixels_mut() {
        let a = p.alpha() as u32;
        if a > 0 {
            let c = |v: u8| (v as u32 * a / 255) as u8;
            if let Some(t) = tiny_skia::PremultipliedColorU8::from_rgba(c(rgb[0]), c(rgb[1]), c(rgb[2]), p.alpha()) {
                *p = t;
            }
        }
    }
    blit(dst, &tinted, x, y, 1.0);
}

/// Altura da ponta dos menus e painéis.
pub const ARROW_H: f32 = 8.0;

/// Ponta de menu/painel mostrando de onde ele vem: bico em (`cx`, `edge` ± `ARROW_H`),
/// para baixo (`down`) ou para cima, saindo da borda `edge` do painel. Pinta com a cor
/// do painel (`rgba`, não premultiplicado) e cobre a borda dele na emenda; o contorno
/// fica só nos dois lados inclinados.
pub fn menu_arrow(pixmap: &mut Pixmap, cx: f32, edge: f32, down: bool, rgba_fill: [u8; 4]) {
    let dir = if down { 1.0 } else { -1.0 };
    arrow(pixmap, |u, v| (cx + u, edge + v * dir), rgba_fill);
}

/// Ponta na lateral do painel: bico em (`edge` ± `ARROW_H`, `cy`), para a esquerda
/// (`left`, o painel está à direita de quem o abriu) ou para a direita.
pub fn menu_arrow_side(pixmap: &mut Pixmap, cy: f32, edge: f32, left: bool, rgba_fill: [u8; 4]) {
    let dir = if left { -1.0 } else { 1.0 };
    arrow(pixmap, |u, v| (edge + v * dir, cy + u), rgba_fill);
}

/// Desenha a ponta em coordenadas locais: `u` ao longo da borda do painel, `v` para
/// fora dele; `at` leva para a tela.
fn arrow(pixmap: &mut Pixmap, at: impl Fn(f32, f32) -> (f32, f32), rgba_fill: [u8; 4]) {
    let half = 9.0;
    let point = |pb: &mut PathBuilder, u: f32, v: f32| {
        let (x, y) = at(u, v);
        pb.line_to(x, y);
    };
    let quad = |pb: &mut PathBuilder, (u1, v1): (f32, f32), (u2, v2): (f32, f32)| {
        let ((x1, y1), (x2, y2)) = (at(u1, v1), at(u2, v2));
        pb.quad_to(x1, y1, x2, y2);
    };
    let sides = |pb: &mut PathBuilder| {
        let (x, y) = at(-half, 0.0);
        pb.move_to(x, y);
        quad(pb, (-2.0, 1.5), (0.0, ARROW_H));
        quad(pb, (2.0, 1.5), (half, 0.0));
    };
    let mut paint = Paint::default();
    paint.anti_alias = true;
    let mut pb = PathBuilder::new();
    sides(&mut pb);
    if let Some(edge_path) = pb.finish() {
        paint.set_color(Color::from_rgba8(0, 0, 0, 40));
        pixmap.stroke_path(&edge_path, &paint, &tiny_skia::Stroke { width: 1.0, ..Default::default() }, tf(), None);
    }
    // O corpo entra 2 px no painel: apaga a borda ali e pinta com a cor do painel.
    let mut pb = PathBuilder::new();
    sides(&mut pb);
    point(&mut pb, half - 0.5, -2.0);
    point(&mut pb, -half + 0.5, -2.0);
    pb.close();
    if let Some(body) = pb.finish() {
        let mut clear = Paint::default();
        clear.anti_alias = true;
        clear.blend_mode = tiny_skia::BlendMode::Clear;
        pixmap.fill_path(&body, &clear, FillRule::Winding, tf(), None);
        paint.set_color(Color::from_rgba8(rgba_fill[0], rgba_fill[1], rgba_fill[2], rgba_fill[3]));
        pixmap.fill_path(&body, &paint, FillRule::Winding, tf(), None);
    }
}

/// Garante pixels premultiplicados válidos (cor nunca maior que a opacidade). Filtros
/// de escala podem passar do ponto nas bordas; um pixel inválido clareia demais quando
/// o compositor aplica transparência (aparece como ponto branco).
pub fn fix_premultiplied(pixmap: &mut Pixmap) {
    for px in pixmap.data_mut().chunks_exact_mut(4) {
        let a = px[3];
        px[0] = px[0].min(a);
        px[1] = px[1].min(a);
        px[2] = px[2].min(a);
    }
}

/// Copia o pixmap (RGBA premultiplicado) para um buffer wl_shm Argb8888 (B, G, R, A).
/// Troca R e B de 4 em 4 bytes como um número só (bem mais rápido que byte a byte).
pub fn to_argb8888(pixmap: &Pixmap, canvas: &mut [u8]) {
    argb_row(pixmap.data(), canvas);
}

/// Converte um trecho de pixels RGBA premultiplicados para Argb8888 (B, G, R, A).
pub fn argb_row(src: &[u8], canvas: &mut [u8]) {
    for (dst, src) in canvas.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        let v = u32::from_le_bytes([src[0], src[1], src[2], src[3]]);
        let swapped = (v & 0xFF00_FF00) | ((v & 0x0000_00FF) << 16) | ((v >> 16) & 0x0000_00FF);
        dst.copy_from_slice(&swapped.to_le_bytes());
    }
}

/// Preenche o pixmap inteiro com um gradiente vertical, linha a linha: uma cor por
/// linha, copiada ao longo dela. Para fundos de tela cheia é dezenas de vezes mais
/// rápido que o gradiente genérico do tiny-skia.
pub fn fill_vertical_fast(pixmap: &mut Pixmap, top: [u8; 4], bottom: [u8; 4]) {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let pixels = pixmap.pixels_mut();
    for y in 0..h {
        let t = if h > 1 { y as f32 / (h - 1) as f32 } else { 0.0 };
        let mix = |a: u8, b: u8| a as f32 + (b as f32 - a as f32) * t;
        let alpha = mix(top[3], bottom[3]);
        let k = alpha / 255.0;
        let color = tiny_skia::PremultipliedColorU8::from_rgba(
            (mix(top[0], bottom[0]) * k).round() as u8,
            (mix(top[1], bottom[1]) * k).round() as u8,
            (mix(top[2], bottom[2]) * k).round() as u8,
            alpha.round() as u8,
        );
        if let Some(color) = color {
            pixels[y * w..(y + 1) * w].fill(color);
        }
    }
}

/// Cor a partir de RGBA em bytes.
pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::from_rgba8(r, g, b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pintura_solida_igual_a_apagar_e_preencher() {
        let mut a = Pixmap::new(60, 40).unwrap();
        a.fill(tiny_skia::Color::from_rgba8(30, 30, 30, 120));
        let mut b = a.clone();
        clear_rounded(&mut a, 5.0, 4.0, 48.0, 30.0, 9.0);
        fill_rounded(&mut a, 5.0, 4.0, 48.0, 30.0, 9.0, tiny_skia::Color::from_rgba8(248, 248, 250, 196));
        paint_rounded_solid(&mut b, 5.0, 4.0, 48.0, 30.0, 9.0, [248, 248, 250, 196]);
        // Miolo idêntico. Nas bordas antisserrilhadas a pintura direta interpola pela
        // cobertura; apagar e depois pintar (tiny-skia) reduz o fundo duas vezes, então
        // ali a diferença pode chegar a ~25%.
        let px = |p: &Pixmap, x: u32, y: u32| p.pixel(x, y).unwrap();
        assert_eq!(px(&a, 30, 20), px(&b, 30, 20));
        assert_eq!(px(&a, 2, 2), px(&b, 2, 2), "fora fica como estava");
        for (p, q) in a.pixels().iter().zip(b.pixels()) {
            assert!((p.alpha() as i32 - q.alpha() as i32).abs() <= 64, "{} vs {}", p.alpha(), q.alpha());
        }
    }

    #[test]
    fn pixels_premultiplicados_validos() {
        let mut p = Pixmap::new(2, 1).unwrap();
        // Pixel inválido (cor 200 com opacidade 100), escrito à mão.
        p.data_mut().copy_from_slice(&[200, 50, 120, 100, 10, 10, 10, 255]);
        fix_premultiplied(&mut p);
        assert_eq!(p.data(), &[100, 50, 100, 100, 10, 10, 10, 255]);
    }

    #[test]
    fn blit_igual_ao_draw_pixmap() {
        // Ícone com miolo opaco, borda semitransparente e cantos vazios.
        let mut icon = Pixmap::new(8, 8).unwrap();
        icon.fill(tiny_skia::Color::from_rgba8(200, 40, 40, 120));
        if let Some(path) = rounded_rect(1.0, 1.0, 6.0, 6.0, 2.0) {
            let mut paint = Paint::default();
            paint.set_color(tiny_skia::Color::from_rgba8(20, 120, 220, 255));
            icon.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
        }
        for opacity in [1.0, 0.35] {
            let mut a = Pixmap::new(12, 12).unwrap();
            a.fill(tiny_skia::Color::from_rgba8(240, 240, 245, 255));
            let mut b = a.clone();
            let paint = tiny_skia::PixmapPaint { opacity, ..Default::default() };
            a.draw_pixmap(-2, 6, icon.as_ref(), &paint, Transform::identity(), None);
            blit(&mut b, &icon, -2, 6, opacity);
            for (p, q) in a.pixels().iter().zip(b.pixels()) {
                for (u, v) in [(p.red(), q.red()), (p.green(), q.green()), (p.blue(), q.blue()), (p.alpha(), q.alpha())] {
                    assert!((u as i32 - v as i32).abs() <= 2, "{u} vs {v} (opacidade {opacity})");
                }
            }
        }
    }

    #[test]
    fn conversao_troca_vermelho_e_azul() {

        let mut p = Pixmap::new(1, 1).unwrap();
        p.fill(Color::from_rgba8(10, 20, 30, 255));
        let mut out = [0u8; 4];
        to_argb8888(&p, &mut out);
        assert_eq!(out, [30, 20, 10, 255]);
    }

    #[test]
    fn gradiente_rapido_igual_nas_pontas() {
        let mut p = Pixmap::new(4, 3).unwrap();
        fill_vertical_fast(&mut p, [255, 0, 0, 255], [0, 0, 255, 255]);
        assert_eq!(p.pixel(0, 0).unwrap().red(), 255);
        assert_eq!(p.pixel(3, 2).unwrap().blue(), 255);
        assert_eq!(p.pixel(2, 1).unwrap().red(), p.pixel(0, 1).unwrap().red());
    }
}

#[cfg(test)]
mod fast_shapes {
    use super::*;

    fn old_fill(p: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
        let path = rounded_rect(x, y, w, h, r).unwrap();
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        p.fill_path(&path, &paint, FillRule::Winding, tf(), None);
    }

    fn max_diff(a: &Pixmap, b: &Pixmap) -> (u8, usize) {
        let mut worst = 0u8;
        let mut many = 0;
        for (x, y) in a.data().iter().zip(b.data()) {
            let d = x.abs_diff(*y);
            worst = worst.max(d);
            if d > 40 {
                many += 1;
            }
        }
        (worst, many)
    }

    #[test]
    fn retangulo_arredondado_igual_ao_do_tiny_skia() {
        for (scale, color) in [(1.0, Color::from_rgba8(30, 120, 250, 255)), (1.5, Color::from_rgba8(255, 255, 255, 120))] {
            set_scale(scale);
            let mut a = Pixmap::new(200, 120).unwrap();
            let mut b = Pixmap::new(200, 120).unwrap();
            a.fill(Color::from_rgba8(40, 40, 40, 255));
            b.fill(Color::from_rgba8(40, 40, 40, 255));
            fill_rounded(&mut a, 10.3, 8.0, 90.0, 40.0, 12.0, color);
            old_fill(&mut b, 10.3, 8.0, 90.0, 40.0, 12.0, color);
            let (worst, many) = max_diff(&a, &b);
            // Só a borda suave pode diferir, e pouco.
            assert!(worst < 110 && many < 40, "escala {scale}: diferença {worst}, {many} pixels");
        }
        set_scale(1.0);
    }

    #[test]
    fn cola_imagem_com_transparencia() {
        let mut dst = Pixmap::new(4, 1).unwrap();
        dst.fill(Color::from_rgba8(0, 0, 255, 255));
        let mut src = Pixmap::new(4, 1).unwrap();
        src.fill(Color::from_rgba8(255, 0, 0, 128));
        blit_px(&mut dst, &src, 0, 0, 1.0);
        let p = dst.pixels()[0];
        assert!((p.red() as i32 - 128).abs() <= 1 && (p.blue() as i32 - 127).abs() <= 2 && p.alpha() == 255, "{p:?}");
        // Opaco copia; meia opacidade mistura.
        let mut dst2 = Pixmap::new(2, 1).unwrap();
        let mut red = Pixmap::new(2, 1).unwrap();
        red.fill(Color::from_rgba8(255, 0, 0, 255));
        blit_px(&mut dst2, &red, 0, 0, 1.0);
        assert_eq!(dst2.pixels()[0].red(), 255);
        let mut dst3 = Pixmap::new(2, 1).unwrap();
        blit_px(&mut dst3, &red, 0, 0, 0.5);
        assert!((dst3.pixels()[0].alpha() as i32 - 128).abs() <= 1);
    }

    #[test]
    fn contorno_so_na_borda() {
        set_scale(1.0);
        let mut p = Pixmap::new(60, 40).unwrap();
        stroke_rounded(&mut p, 10.5, 10.5, 39.0, 19.0, 6.0, 1.0, Color::from_rgba8(0, 0, 0, 255));
        // Meio vazio, borda pintada.
        assert_eq!(p.pixel(30, 20).unwrap().alpha(), 0);
        assert!(p.pixel(30, 10).unwrap().alpha() > 200);
        assert!(p.pixel(10, 20).unwrap().alpha() > 200);
    }
}

