//! Opt-in timings for desktop checks; omitted from production builds.
use std::{cell::RefCell, collections::BTreeMap, time::Instant};
use windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

pub(super) const REPORT_MESSAGE: u32 = WM_APP + 20;
thread_local! { static SAMPLES: RefCell<BTreeMap<&'static str, Vec<f64>>> = const { RefCell::new(BTreeMap::new()) }; }

pub(super) struct Scope {
    name: &'static str,
    started: Option<Instant>,
}
impl Scope {
    pub(super) fn new(name: &'static str) -> Self {
        Self {
            name,
            started: std::env::var_os("MAGIC_LANGUAGE_SWITCHER_UI_PROFILE").map(|_| Instant::now()),
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            SAMPLES.with_borrow_mut(|samples| samples.entry(self.name).or_default().push(elapsed));
        }
    }
}
pub(super) fn record(name: &'static str, elapsed: f64) {
    if std::env::var_os("MAGIC_LANGUAGE_SWITCHER_UI_PROFILE").is_some() {
        SAMPLES.with_borrow_mut(|samples| samples.entry(name).or_default().push(elapsed));
    }
}
pub(super) fn flush() {
    let Some(path) = std::env::var_os("MAGIC_LANGUAGE_SWITCHER_UI_PROFILE") else {
        return;
    };
    let report = SAMPLES.with_borrow(|samples| samples.iter().map(|(name, values)| {
        let mut sorted=values.clone(); sorted.sort_by(f64::total_cmp);
        let percentile=|p:f64| sorted[((sorted.len()-1) as f64*p).round() as usize];
        (*name, serde_json::json!({"count":sorted.len(),"median_ms":percentile(0.5),"p95_ms":percentile(0.95),"max_ms":sorted.last()}))
    }).collect::<BTreeMap<_,_>>());
    if let Ok(json) = serde_json::to_vec_pretty(&report) {
        let _ = std::fs::write(path, json);
    }
}
