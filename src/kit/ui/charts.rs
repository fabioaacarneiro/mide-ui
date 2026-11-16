//! Gráficos: o histórico de valores que anda da direita para a esquerda (CPU, memória,
//! disco e rede do Monitor de Recursos, como os do Monitor de Atividade do Mac).

use super::{color, fill_rounded, Response, Ui};
use crate::kit::theme::Rgba;

impl Ui<'_> {
    /// Gráfico de histórico na largura toda e na altura `height`: cada série é
    /// (valores do mais antigo ao mais novo, cor); o mais novo fica na borda direita.
    /// `max` é o topo do gráfico; com `stacked`, as séries se empilham (sistema +
    /// usuário no gráfico de CPU), senão cada uma é uma linha com área por baixo
    /// (recebidos e enviados na rede). Fundo num quadro arredondado, com linhas de
    /// guia leves.
    pub fn history_graph(&mut self, series: &[(&[f32], Rgba)], max: f32, stacked: bool, height: f32) -> Response {
        let id = self.ctx.id();
        let rect = self.alloc(None, height);
        let r = self.interact(id, rect);
        let t = self.ctx.theme.clone();
        let (x, y, w, h) = rect;
        fill_rounded(self.canvas, x, y, w, h, 8.0, color(t.shade(if t.dark { 10 } else { 6 })));
        // Guias em 25, 50 e 75%.
        let guide = if t.dark { [255, 255, 255, 18] } else { [0, 0, 0, 14] };
        for i in 1..4 {
            fill_rounded(self.canvas, x + 6.0, y + h * i as f32 / 4.0, w - 12.0, 1.0, 0.0, color(guide));
        }
        let pad = 4.0;
        let (gx, gy, gw, gh) = (x + pad, y + pad, (w - 2.0 * pad).max(1.0), (h - 2.0 * pad).max(1.0));
        let n = series.iter().map(|(v, _)| v.len()).max().unwrap_or(0);
        if n < 2 || max <= 0.0 {
            return r;
        }
        let step = gw / (n - 1) as f32;
        let mut base = vec![0.0f32; n];
        let k = crate::ui::draw::scale();
        for (values, c) in series {
            // Alinhadas pela direita: a mais curta começa mais tarde.
            let offset = n - values.len();
            let top: Vec<f32> = (0..n)
                .map(|i| {
                    let v = if i >= offset { values[i - offset].max(0.0) } else { 0.0 };
                    if stacked { base[i] + v } else { v }
                })
                .collect();
            let point = |i: usize, v: f32| ((gx + i as f32 * step) * k, (gy + gh - (v / max).min(1.0) * gh) * k);
            let mut area = tiny_skia::PathBuilder::new();
            let mut line = tiny_skia::PathBuilder::new();
            for (i, v) in top.iter().enumerate() {
                let (px, py) = point(i, *v);
                if i == 0 {
                    area.move_to(px, py);
                    line.move_to(px, py);
                } else {
                    area.line_to(px, py);
                    line.line_to(px, py);
                }
            }
            for i in (0..n).rev() {
                let (px, py) = point(i, if stacked { base[i] } else { 0.0 });
                area.line_to(px, py);
            }
            area.close();
            let mut paint = tiny_skia::Paint { anti_alias: true, ..Default::default() };
            if let Some(path) = area.finish() {
                paint.set_color(color([c[0], c[1], c[2], 90]));
                self.canvas.fill_path(&path, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
            }
            if let Some(path) = line.finish() {
                paint.set_color(color([c[0], c[1], c[2], 255]));
                self.canvas.stroke_path(&path, &paint, &tiny_skia::Stroke { width: 1.5 * k, ..Default::default() }, tiny_skia::Transform::identity(), None);
            }
            if stacked {
                base = top;
            }
        }
        r
    }
}
