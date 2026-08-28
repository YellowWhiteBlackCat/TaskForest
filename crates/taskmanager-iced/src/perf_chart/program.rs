//! Iced Canvas program implementation for the Performance chart.

use super::*;
use iced::{Renderer, Theme};

impl canvas::Program<Message> for PerfChart {
    type State = PerfChartState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let sample_count = self.cpu.len().max(self.memory.len());
                let next = cursor
                    .position_over(bounds)
                    .and_then(|position| hovered_index(position.x, bounds.width, sample_count));
                if next != state.hover.index {
                    state.hover.index = next;
                    return Some(canvas::Action::request_redraw());
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorLeft) if state.hover.index.is_some() => {
                state.hover.index = None;
                return Some(canvas::Action::request_redraw());
            }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        // TWO physically separate caches (the round-1 process_sparkline pattern,
        // extended to a hover chart):
        //  - DATA cache: grid + two series. Cleared only when the data
        //    fingerprint changes (new sample / window shift / smooth toggle).
        //    Cursor motion NEVER busts this — it's the expensive geometry.
        //  - OVERLAY cache: hover readout pill. Cleared when the hover index OR
        //    the data fingerprint changes — so a moved pill never shows a stale
        //    reading (the pill text is re-read from the samples at the hovered
        //    index each rebuild).
        let data_fp = self.fingerprint();
        if *state.data_fingerprint.borrow() != data_fp {
            *state.data_fingerprint.borrow_mut() = data_fp.clone();
            state.data_cache.clear();
        }
        let overlay_fp = PerfChartOverlayFingerprint {
            hover_index: state.hover.index,
            data: data_fp,
        };
        if *state.overlay_fingerprint.borrow() != overlay_fp {
            *state.overlay_fingerprint.borrow_mut() = overlay_fp;
            state.overlay_cache.clear();
        }

        // `Cache::draw` invokes the closure synchronously. Borrow the owned
        // series instead of cloning both history windows on every repaint;
        // the cache already controls when geometry work runs.
        let cpu = &self.cpu;
        let memory = &self.memory;
        let cpu_color = self.cpu_color;
        let memory_color = self.memory_color;
        let grid_color = self.grid_color;
        let smooth = self.smooth;
        let data_geometry = state.data_cache.draw(renderer, bounds.size(), |frame| {
            let size = frame.size();
            draw_grid_opts(frame, size, grid_color, ChartOpts::DEFAULT);
            // CPU first (drawn under memory), then memory, so the upper series
            // sits on top at any crossing. Each series is drawn from its own
            // samples — a partial buffer never fabricates points for the other.
            draw_series(frame, cpu, size, cpu_color, smooth);
            draw_series(frame, memory, size, memory_color, smooth);
        });

        let hover_index = state.hover.index;
        let readout = self.readout;
        let cpu_color = self.cpu_color;
        let memory_color = self.memory_color;
        let overlay_geometry = state.overlay_cache.draw(renderer, bounds.size(), |frame| {
            let Some(index) = hover_index else {
                return;
            };
            let size = frame.size();
            draw_hover_readout(
                frame,
                size,
                index,
                &[(cpu.as_ref(), cpu_color), (memory.as_ref(), memory_color)],
                grid_color,
                readout,
            );
        });

        vec![data_geometry, overlay_geometry]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.position_over(bounds).is_some() {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}
