use crate::config::{Config, Theme};
use crate::grep::{apply_replace, build_regex, count_total_matches, search};
use crate::history::History;
use crate::models::{FileMatch, LineMatch, MatchRange, SearchParams, SearchResult, ViewMode};
use chrono::Local;
use egui::{
    text::LayoutJob, Color32, CornerRadius, FontId, Margin, RichText, ScrollArea, Stroke,
    TextFormat, Ui, Vec2,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

type ReplacePreview = Option<(crate::models::SearchParams, Vec<(PathBuf, String, String)>)>;

// ── Color palette ─────────────────────────────────────────────────────────────
// All Color32 fields are Copy, so Pal is Copy too.
#[derive(Clone, Copy)]
pub struct Pal {
    pub bg_base: Color32,
    pub bg_mantle: Color32,
    pub bg_surface0: Color32,
    pub bg_surface1: Color32,
    pub bg_overlay0: Color32,
    pub text: Color32,
    pub subtext: Color32,
    pub muted: Color32,
    pub placeholder: Color32, // hint text in empty inputs (intentionally very dim)
    pub accent: Color32,
    pub yellow: Color32,
    pub green: Color32,
    pub red: Color32,
    pub match_bg: Color32,
    pub match_text: Color32, // text color for highlighted match words
    pub is_dark: bool,
}

impl Pal {
    fn dark() -> Self {
        Self {
            bg_base: Color32::from_rgb(30, 30, 46),
            bg_mantle: Color32::from_rgb(24, 24, 37),
            bg_surface0: Color32::from_rgb(49, 50, 68),
            bg_surface1: Color32::from_rgb(69, 71, 90),
            bg_overlay0: Color32::from_rgb(108, 112, 134),
            text: Color32::from_rgb(205, 214, 244),
            subtext: Color32::from_rgb(166, 173, 200),
            muted: Color32::from_rgb(88, 91, 112),
            placeholder: Color32::from_rgb(72, 74, 96), // ~2.1:1 on bg_base (clearly dim hint)
            accent: Color32::from_rgb(137, 180, 250),
            yellow: Color32::from_rgb(249, 226, 175),
            green: Color32::from_rgb(166, 227, 161),
            red: Color32::from_rgb(243, 139, 168),
            match_bg: Color32::from_rgba_unmultiplied(249, 226, 175, 75),
            match_text: Color32::from_rgb(249, 226, 175),
            is_dark: true,
        }
    }

    fn light() -> Self {
        Self {
            bg_base: Color32::from_rgb(239, 241, 245),
            bg_mantle: Color32::from_rgb(230, 233, 239),
            bg_surface0: Color32::from_rgb(204, 208, 218),
            bg_surface1: Color32::from_rgb(188, 192, 204),
            bg_overlay0: Color32::from_rgb(156, 160, 176),
            text: Color32::from_rgb(76, 79, 105),
            subtext: Color32::from_rgb(92, 95, 119),
            muted: Color32::from_rgb(120, 124, 148), // darkened from (156,160,176): 3.6:1 on bg_base
            placeholder: Color32::from_rgb(165, 168, 185), // ~2.1:1 on bg_base (clearly dim hint)
            accent: Color32::from_rgb(18, 75, 200), // darkened from (30,102,245): 4.9:1 on bg_surface0
            yellow: Color32::from_rgb(223, 142, 29),
            green: Color32::from_rgb(64, 160, 43),
            red: Color32::from_rgb(210, 15, 57),
            match_bg: Color32::from_rgba_unmultiplied(223, 142, 29, 95),
            match_text: Color32::from_rgb(135, 70, 0), // dark amber: 4.9:1 on yellow match_bg
            is_dark: false,
        }
    }

    fn high_contrast() -> Self {
        Self {
            bg_base: Color32::from_rgb(0, 0, 0),
            bg_mantle: Color32::from_rgb(10, 10, 15),
            bg_surface0: Color32::from_rgb(25, 25, 35),
            bg_surface1: Color32::from_rgb(42, 42, 58),
            bg_overlay0: Color32::from_rgb(80, 80, 100),
            text: Color32::from_rgb(255, 255, 255),
            subtext: Color32::from_rgb(220, 220, 235),
            muted: Color32::from_rgb(130, 130, 160),
            placeholder: Color32::from_rgb(80, 80, 100), // ~2.7:1 on bg_base (clearly dim hint)
            accent: Color32::from_rgb(100, 200, 255),
            yellow: Color32::from_rgb(255, 235, 80),
            green: Color32::from_rgb(80, 255, 120),
            red: Color32::from_rgb(255, 90, 110),
            match_bg: Color32::from_rgba_unmultiplied(255, 235, 80, 100),
            match_text: Color32::from_rgb(255, 235, 80),
            is_dark: true,
        }
    }

    pub fn from_theme(t: &Theme, ctx: &egui::Context) -> Self {
        match t {
            Theme::System => {
                let is_light = ctx.input(|i| i.raw.system_theme == Some(egui::Theme::Light));
                if is_light {
                    Self::light()
                } else {
                    Self::dark()
                }
            }
            Theme::Dark => Self::dark(),
            Theme::Light => Self::light(),
            Theme::HighContrast => Self::high_contrast(),
        }
    }
}

/// Design tokens — spacing scale, corner radii. All layout constants live here.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct Tok {
    // Spacing scale (points)
    pub sp2: f32,  // tight gap / icon pad
    pub sp4: f32,  // gutter, small pad
    pub sp6: f32,  // inner component gap
    pub sp8: f32,  // default item spacing
    pub sp10: f32, // button horizontal padding
    pub sp12: f32, // panel inner margin (horizontal)
    pub sp16: f32, // window margin / panel separation
    // Corner radii
    pub r_sm: f32, // 3 – chip, density bar
    pub r_md: f32, // 4 – button, input, widget
    pub r_lg: f32, // 8 – window, floating panel
}

impl Tok {
    pub fn new() -> Self {
        Self {
            sp2: 2.0,
            sp4: 4.0,
            sp6: 6.0,
            sp8: 8.0,
            sp10: 10.0,
            sp12: 12.0,
            sp16: 16.0,
            r_sm: 3.0,
            r_md: 4.0,
            r_lg: 8.0,
        }
    }
}

// ── Search state ─────────────────────────────────────────────────────────────
enum SearchState {
    Idle,
    Running,
    Done(u64, bool), // (duration_ms, truncated)
    Error(String),
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum FocusedPane {
    FileList,
    Content,
}

#[derive(Clone, Debug)]
enum PaletteAction {
    FocusPattern,
    FocusDir,
    NewTab,
    ToggleHistory,
    ToggleSettings,
    ToggleReplace,
    SwitchTheme(Theme),
    RerunSearch,
    ClearResults,
    SetDir(String),
    SetPattern(String),
    ShowShortcuts,
}

// ── Result tab ────────────────────────────────────────────────────────────────
#[derive(Clone)]
struct ResultTab {
    is_settings: bool,
    result: Option<SearchResult>,
    selected_files: BTreeSet<PathBuf>,
    collapsed_files: BTreeSet<PathBuf>,
    view_mode: ViewMode,
    file_filter: String,
    current_match: Option<(usize, usize)>,
    scroll_to_current: bool,
}

// ── App ───────────────────────────────────────────────────────────────────────
pub struct GrepApp {
    params: SearchParams,
    config: Config,
    history: History,

    search_state: Arc<Mutex<SearchState>>,
    current_result: Option<SearchResult>,
    tabs: Vec<ResultTab>,
    active_tab: Option<usize>,
    cancel_flag: Option<Arc<std::sync::atomic::AtomicBool>>,

    selected_files: BTreeSet<PathBuf>,
    collapsed_files: BTreeSet<PathBuf>,
    view_mode: ViewMode,
    file_filter: String,

    show_history: bool,
    show_replace: bool,
    show_palette: bool,
    palette_query: String,
    palette_selected: usize,
    palette_focus: bool,
    palette_instance: u32,
    // (params_at_preview_time, [(path, original, preview)])
    replace_preview: ReplacePreview,

    show_replace_confirm: bool,
    replace_confirm_files: usize,
    replace_confirm_matches: usize,
    replace_confirm_snapshot: Option<Vec<FileMatch>>,
    show_shortcuts: bool,

    focused_pane: FocusedPane,
    current_match: Option<(usize, usize)>, // (file_index, line_match_index)
    scroll_to_current: bool,

    pal: Pal,
    tok: Tok,
    applied_theme: Theme,
    applied_font_size: f32,
    applied_font_path: String,

    status_msg: String,
    copied_flash: Option<std::time::Instant>,
    copied_file_flash: Option<(PathBuf, std::time::Instant)>,
    focus_pattern: bool,
    focus_dir: bool,
    pat_suppress_popup_open: bool,
    dir_suggest_idx: Option<usize>,
    pat_suggest_idx: Option<usize>,
    inc_suggest_idx: Option<usize>,
    exc_suggest_idx: Option<usize>,
    history_filter: String,
    settings_tab: u8,

    search_scanned: Arc<std::sync::atomic::AtomicUsize>,
    search_hits: Arc<std::sync::atomic::AtomicUsize>,
    search_total: Arc<std::sync::atomic::AtomicUsize>,
    search_result_rx: Option<std::sync::mpsc::Receiver<FileMatch>>,
    search_live_files: Vec<FileMatch>,
    preset_new_name: String,
    preset_new_glob: String,
    editing_preset_idx: Option<usize>,
    dnd_hovered_preset_idx: Option<usize>,
    last_search_error: Option<String>,
}

impl GrepApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = Config::load();

        let backup_dir = config.backup_dir.clone();
        let retention_days = config.backup_retention_days;
        std::thread::spawn(move || {
            let _ = crate::grep::cleanup_expired_backups(&backup_dir, retention_days);
        });

        let pal = Pal::from_theme(&config.theme, &cc.egui_ctx);
        apply_theme(&cc.egui_ctx, pal, Tok::new());
        apply_font_size(&cc.egui_ctx, config.font_size);
        let applied_font_path = config.custom_font_path.clone();
        setup_fonts(&cc.egui_ctx, &applied_font_path);

        let applied_theme = config.theme.clone();
        let applied_font_size = config.font_size;
        let history = History::load(config.history_limit);

        Self {
            params: SearchParams::default(),
            config,
            history,
            search_state: Arc::new(Mutex::new(SearchState::Idle)),
            current_result: None,
            tabs: Vec::new(),
            active_tab: None,
            cancel_flag: None,
            selected_files: BTreeSet::new(),
            collapsed_files: BTreeSet::new(),
            view_mode: ViewMode::Tree,
            file_filter: String::new(),
            show_history: false,
            show_replace: false,
            show_palette: false,
            palette_query: String::new(),
            palette_selected: 0,
            palette_focus: false,
            palette_instance: 0,
            replace_preview: None, // (params, [(path, original, preview)])
            show_replace_confirm: false,
            replace_confirm_files: 0,
            replace_confirm_matches: 0,
            replace_confirm_snapshot: None,
            show_shortcuts: false,
            focused_pane: FocusedPane::FileList,
            current_match: None,
            scroll_to_current: false,
            pal,
            tok: Tok::new(),
            applied_theme,
            applied_font_size,
            applied_font_path,
            status_msg: "Ready".to_string(),
            copied_flash: None,
            copied_file_flash: None,
            focus_pattern: false,
            focus_dir: false,
            pat_suppress_popup_open: false,
            dir_suggest_idx: None,
            pat_suggest_idx: None,
            inc_suggest_idx: None,
            exc_suggest_idx: None,
            history_filter: String::new(),
            settings_tab: 0,
            search_scanned: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            search_hits: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            search_total: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            search_result_rx: None,
            search_live_files: Vec::new(),
            preset_new_name: String::new(),
            preset_new_glob: String::new(),
            editing_preset_idx: None,
            dnd_hovered_preset_idx: None,
            last_search_error: None,
        }
    }

    fn ensure_theme_applied(&mut self, ctx: &egui::Context) {
        let mut theme_changed = self.config.theme != self.applied_theme;
        if self.config.theme == Theme::System {
            let current_is_light = ctx.input(|i| i.raw.system_theme == Some(egui::Theme::Light));
            if current_is_light == self.pal.is_dark {
                theme_changed = true;
            }
        }
        let size_changed = (self.config.font_size - self.applied_font_size).abs() > 0.01;
        let font_path_changed = self.config.custom_font_path != self.applied_font_path;

        if theme_changed {
            self.pal = Pal::from_theme(&self.config.theme, ctx);
            apply_theme(ctx, self.pal, self.tok);
            self.applied_theme = self.config.theme.clone();
        }
        if size_changed {
            apply_font_size(ctx, self.config.font_size);
            self.applied_font_size = self.config.font_size;
        }
        if font_path_changed {
            setup_fonts(ctx, &self.config.custom_font_path);
            self.applied_font_path = self.config.custom_font_path.clone();
        }
    }

    // ── Tab management ────────────────────────────────────────────────────────

    // SYNC CONTRACT: the six fields below must stay identical in
    // save_active_tab (mirror→tab), switch_to_tab (tab→mirror), and
    // update_active_tab (mirror→new-tab). Add new per-tab nav fields here first.
    fn save_active_tab(&mut self) {
        if let Some(idx) = self.active_tab {
            if let Some(tab) = self.tabs.get_mut(idx) {
                tab.selected_files = self.selected_files.clone();
                tab.collapsed_files = self.collapsed_files.clone();
                tab.view_mode = self.view_mode.clone();
                tab.file_filter = self.file_filter.clone();
                tab.current_match = self.current_match;
                tab.scroll_to_current = self.scroll_to_current;
            }
        }
    }

    fn switch_to_tab(&mut self, idx: usize) {
        self.save_active_tab();
        let tab = &self.tabs[idx];
        self.current_result = tab.result.clone();
        self.selected_files = tab.selected_files.clone();
        self.collapsed_files = tab.collapsed_files.clone();
        self.view_mode = tab.view_mode.clone();
        self.file_filter = tab.file_filter.clone();
        self.current_match = tab.current_match;
        self.scroll_to_current = tab.scroll_to_current;
        if let Some(result) = &tab.result {
            self.params = result.params.clone();
        }
        self.active_tab = Some(idx);
    }

    /// Switch to an existing empty tab, or open a new one if none exists.
    fn ensure_empty_tab(&mut self) {
        if self.current_result.is_none() {
            return;
        }
        if let Some(idx) = self
            .tabs
            .iter()
            .position(|t| t.result.is_none() && !t.is_settings)
        {
            self.switch_to_tab(idx);
            return;
        }
        self.new_empty_tab();
    }

    fn new_empty_tab(&mut self) {
        self.save_active_tab();
        let tab = ResultTab {
            is_settings: false,
            result: None,
            selected_files: BTreeSet::new(),
            collapsed_files: BTreeSet::new(),
            view_mode: ViewMode::Tree,
            file_filter: String::new(),
            current_match: None,
            scroll_to_current: false,
        };
        self.tabs.push(tab);
        self.active_tab = Some(self.tabs.len() - 1);
        self.current_result = None;
        self.selected_files.clear();
        self.collapsed_files.clear();
        self.file_filter.clear();
        self.current_match = None;
        self.scroll_to_current = false;
    }

    fn update_active_tab(&mut self, result: SearchResult) {
        self.current_result = Some(result.clone());
        if let Some(idx) = self.active_tab {
            if let Some(tab) = self.tabs.get_mut(idx) {
                tab.result = Some(result);
                tab.selected_files = self.selected_files.clone();
                tab.collapsed_files = self.collapsed_files.clone();
                tab.view_mode = self.view_mode.clone();
                tab.file_filter = self.file_filter.clone();
                tab.current_match = self.current_match;
                tab.scroll_to_current = self.scroll_to_current;
            }
        } else {
            let tab = ResultTab {
                is_settings: false,
                result: Some(result),
                selected_files: self.selected_files.clone(),
                collapsed_files: self.collapsed_files.clone(),
                view_mode: self.view_mode.clone(),
                file_filter: self.file_filter.clone(),
                current_match: self.current_match,
                scroll_to_current: self.scroll_to_current,
            };
            self.tabs.push(tab);
            self.active_tab = Some(0);
        }
    }

    fn close_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        self.tabs.remove(idx);
        if self.tabs.is_empty() {
            self.active_tab = None;
            self.current_result = None;
            self.selected_files.clear();
            self.collapsed_files.clear();
            self.view_mode = ViewMode::Tree;
            self.file_filter.clear();
            self.current_match = None;
            self.scroll_to_current = false;
        } else {
            let n = self.tabs.len();
            let new_idx = self
                .active_tab
                .map(|ai| if ai >= n { n - 1 } else { ai })
                .unwrap_or(0);
            self.active_tab = None; // skip save in switch_to_tab
            self.switch_to_tab(new_idx);
        }
    }

    fn is_settings_active(&self) -> bool {
        self.active_tab
            .and_then(|i| self.tabs.get(i))
            .map(|t| t.is_settings)
            .unwrap_or(false)
    }

    fn open_settings_tab(&mut self) {
        if let Some(idx) = self.tabs.iter().position(|t| t.is_settings) {
            if self.active_tab != Some(idx) {
                self.switch_to_tab(idx);
            }
            return;
        }
        self.save_active_tab();
        let tab = ResultTab {
            is_settings: true,
            result: None,
            selected_files: BTreeSet::new(),
            collapsed_files: BTreeSet::new(),
            view_mode: ViewMode::Tree,
            file_filter: String::new(),
            current_match: None,
            scroll_to_current: false,
        };
        self.tabs.push(tab);
        let new_idx = self.tabs.len() - 1;
        self.active_tab = Some(new_idx);
        self.current_result = None;
        self.selected_files.clear();
        self.collapsed_files.clear();
        self.file_filter.clear();
        self.current_match = None;
        self.scroll_to_current = false;
    }

    fn close_settings_tab(&mut self) {
        if let Some(idx) = self.tabs.iter().position(|t| t.is_settings) {
            self.close_tab(idx);
        }
    }

    fn start_search(&mut self) {
        self.last_search_error = None;
        if self.params.pattern.is_empty() {
            self.status_msg = "Pattern is empty".to_string();
            return;
        }
        if self.params.directory.is_empty() {
            self.status_msg = "No directory selected — enter a path in the Dir field".to_string();
            self.focus_dir = true;
            return;
        }
        if let Err(e) = build_regex(&self.params) {
            self.status_msg = format!("Invalid pattern: {}", e);
            return;
        }

        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.cancel_flag = Some(Arc::clone(&cancel));
        self.search_scanned
            .store(0, std::sync::atomic::Ordering::Relaxed);
        self.search_hits
            .store(0, std::sync::atomic::Ordering::Relaxed);
        self.search_total
            .store(0, std::sync::atomic::Ordering::Relaxed);

        // Save current tab state so it's preserved if the search is cancelled
        self.save_active_tab();

        *self.search_state.lock().unwrap() = SearchState::Running;
        self.status_msg = "Searching...".to_string();
        self.replace_preview = None;

        let params = self.params.clone();
        let config = self.config.clone();
        let state = Arc::clone(&self.search_state);
        let scanned = Arc::clone(&self.search_scanned);
        let hits = Arc::clone(&self.search_hits);
        let total = Arc::clone(&self.search_total);

        let (tx, rx) = std::sync::mpsc::sync_channel::<FileMatch>(500);
        self.search_result_rx = Some(rx);
        self.search_live_files.clear();

        std::thread::spawn(move || {
            let t0 = Instant::now();
            match search(&params, &config, cancel, scanned, hits, total, tx) {
                Ok(truncated) => {
                    let ms = t0.elapsed().as_millis() as u64;
                    *state.lock().unwrap() = SearchState::Done(ms, truncated);
                }
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("cancelled") {
                        *state.lock().unwrap() = SearchState::Cancelled;
                    } else {
                        *state.lock().unwrap() = SearchState::Error(err_str);
                    }
                }
            }
        });
    }

    fn cancel_search(&mut self) {
        if let Some(cancel) = &self.cancel_flag {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            self.status_msg = "Cancelling...".to_string();
        }
    }

    fn drain_search_rx(&mut self) {
        if let Some(rx) = &self.search_result_rx {
            while let Ok(fm) = rx.try_recv() {
                self.search_live_files.push(fm);
            }
        }
    }

    fn finalize_search(&mut self, ms: u64, truncated: bool, cancelled: bool) {
        // Drain any remaining results from the channel
        if let Some(rx) = self.search_result_rx.take() {
            for fm in rx.try_iter() {
                self.search_live_files.push(fm);
            }
        }
        let mut files = std::mem::take(&mut self.search_live_files);
        files.sort_by(|a, b| a.path.cmp(&b.path));

        let total = count_total_matches(&files);
        let trunc_note = if truncated { " (truncated)" } else { "" };
        if cancelled && files.is_empty() {
            self.status_msg = "Search cancelled".to_string();
            return;
        }
        self.status_msg = if cancelled {
            format!(
                "{} matches in {} files  (cancelled){}",
                total,
                files.len(),
                trunc_note
            )
        } else {
            format!(
                "{} matches in {} files  ({} ms){}",
                total,
                files.len(),
                ms,
                trunc_note
            )
        };

        // Clear working set for new results (selected_files was preserved during search)
        self.selected_files.clear();
        self.collapsed_files.clear();
        self.file_filter.clear();

        for f in &files {
            self.selected_files.insert(f.path.clone());
        }
        let result = SearchResult {
            id: self.history.next_id(),
            params: self.params.clone(),
            timestamp: Local::now().to_rfc3339(),
            duration_ms: ms,
            total_matches: total,
            truncated,
            files,
        };
        self.history.push(result.clone());
        self.current_match = if !result.files.is_empty() && !result.files[0].matches.is_empty() {
            let idx = result.files[0]
                .matches
                .iter()
                .position(|m| m.is_match)
                .unwrap_or(0);
            Some((0, idx))
        } else {
            None
        };
        self.scroll_to_current = true;
        if !cancelled {
            self.update_active_tab(result);
        } else {
            self.current_result = Some(result);
        }
    }

    fn poll_search(&mut self) {
        // Always drain partial results so the live counter updates
        self.drain_search_rx();

        let is_done = matches!(
            *self.search_state.lock().unwrap(),
            SearchState::Done(_, _) | SearchState::Error(_) | SearchState::Cancelled
        );
        if !is_done {
            return;
        }

        let state = std::mem::replace(&mut *self.search_state.lock().unwrap(), SearchState::Idle);

        self.cancel_flag = None;

        match state {
            SearchState::Done(ms, truncated) => {
                self.finalize_search(ms, truncated, false);
            }
            SearchState::Error(e) => {
                self.search_result_rx = None;
                self.search_live_files.clear();
                self.status_msg = format!("Error: {}", e);
                self.last_search_error = Some(e);
            }
            SearchState::Cancelled => {
                self.finalize_search(0, false, true);
                // Restore the pre-search state that start_search preserved via
                // save_active_tab. Temporarily clear active_tab so switch_to_tab
                // skips the save step (same pattern as close_tab uses on line ~467),
                // which would otherwise overwrite the saved state with the
                // cancelled-search mirror values.
                if let Some(idx) = self.active_tab {
                    self.active_tab = None;
                    self.switch_to_tab(idx);
                } else {
                    self.current_result = None;
                    self.selected_files.clear();
                    self.current_match = None;
                    self.scroll_to_current = false;
                }
            }
            _ => {}
        }
    }

    fn get_filtered_paths(&self) -> Vec<PathBuf> {
        let Some(result) = &self.current_result else {
            return Vec::new();
        };
        let base = PathBuf::from(&result.params.directory);
        let filter_lower = self.file_filter.to_lowercase();
        if filter_lower.is_empty() {
            result.files.iter().map(|f| f.path.clone()).collect()
        } else {
            result
                .files
                .iter()
                .filter(|f| {
                    let rel = f.path.strip_prefix(&base).unwrap_or(&f.path);
                    rel.to_string_lossy().to_lowercase().contains(&filter_lower)
                })
                .map(|f| f.path.clone())
                .collect()
        }
    }

    fn move_match_next(&mut self) {
        let Some(result) = &self.current_result else {
            return;
        };
        if result.files.is_empty() {
            return;
        }

        let mut all_matches = Vec::new();
        for (f_idx, fm) in result.files.iter().enumerate() {
            for (m_idx, lm) in fm.matches.iter().enumerate() {
                if lm.is_match {
                    all_matches.push((f_idx, m_idx));
                }
            }
        }
        if all_matches.is_empty() {
            return;
        }

        let next_idx = match self.current_match {
            Some(cur) => {
                if let Some(pos) = all_matches.iter().position(|&x| x == cur) {
                    (pos + 1) % all_matches.len()
                } else {
                    0
                }
            }
            None => 0,
        };

        let (f_idx, m_idx) = all_matches[next_idx];
        self.current_match = Some((f_idx, m_idx));
        self.scroll_to_current = true;
        let path = result.files[f_idx].path.clone();
        self.selected_files.insert(path);
    }

    fn move_match_prev(&mut self) {
        let Some(result) = &self.current_result else {
            return;
        };
        if result.files.is_empty() {
            return;
        }

        let mut all_matches = Vec::new();
        for (f_idx, fm) in result.files.iter().enumerate() {
            for (m_idx, lm) in fm.matches.iter().enumerate() {
                if lm.is_match {
                    all_matches.push((f_idx, m_idx));
                }
            }
        }
        if all_matches.is_empty() {
            return;
        }

        let prev_idx = match self.current_match {
            Some(cur) => {
                if let Some(pos) = all_matches.iter().position(|&x| x == cur) {
                    if pos == 0 {
                        all_matches.len() - 1
                    } else {
                        pos - 1
                    }
                } else {
                    all_matches.len() - 1
                }
            }
            None => all_matches.len() - 1,
        };

        let (f_idx, m_idx) = all_matches[prev_idx];
        self.current_match = Some((f_idx, m_idx));
        self.scroll_to_current = true;
        let path = result.files[f_idx].path.clone();
        self.selected_files.insert(path);
    }

    fn move_file_next(&mut self) {
        let paths = self.get_filtered_paths();
        if paths.is_empty() {
            return;
        }

        let cur_idx = paths.iter().position(|p| self.selected_files.contains(p));

        let next_idx = match cur_idx {
            Some(idx) => (idx + 1) % paths.len(),
            None => 0,
        };

        self.selected_files.clear();
        self.selected_files.insert(paths[next_idx].clone());

        if let Some(result) = &self.current_result {
            let actual_f_idx = result
                .files
                .iter()
                .position(|f| f.path == paths[next_idx])
                .unwrap_or(0);
            let first_match_idx = result.files[actual_f_idx]
                .matches
                .iter()
                .position(|m| m.is_match)
                .unwrap_or(0);
            self.current_match = Some((actual_f_idx, first_match_idx));
            self.scroll_to_current = true;
        }
    }

    fn move_file_prev(&mut self) {
        let paths = self.get_filtered_paths();
        if paths.is_empty() {
            return;
        }

        let cur_idx = paths.iter().position(|p| self.selected_files.contains(p));

        let prev_idx = match cur_idx {
            Some(idx) => {
                if idx == 0 {
                    paths.len() - 1
                } else {
                    idx - 1
                }
            }
            None => paths.len() - 1,
        };

        self.selected_files.clear();
        self.selected_files.insert(paths[prev_idx].clone());

        if let Some(result) = &self.current_result {
            let actual_f_idx = result
                .files
                .iter()
                .position(|f| f.path == paths[prev_idx])
                .unwrap_or(0);
            let first_match_idx = result.files[actual_f_idx]
                .matches
                .iter()
                .position(|m| m.is_match)
                .unwrap_or(0);
            self.current_match = Some((actual_f_idx, first_match_idx));
            self.scroll_to_current = true;
        }
    }

    fn open_selected_file_in_editor(&self) {
        let editor_cmd = self.config.editor_command.clone();
        if editor_cmd.trim().is_empty() {
            return;
        }

        if let Some(path) = self.selected_files.iter().next() {
            let line = self.current_match.and_then(|(f_idx, m_idx)| {
                let result = self.current_result.as_ref()?;
                let fm = result.files.get(f_idx)?;
                if &fm.path == path {
                    fm.matches.get(m_idx).map(|m| m.line_number)
                } else {
                    None
                }
            });
            open_in_editor(path, line, &editor_cmd);
        }
    }

    fn load_history_entry(&mut self, result: SearchResult) {
        self.ensure_empty_tab();
        self.params = result.params.clone();
        self.save_active_tab();
        self.selected_files.clear();
        self.collapsed_files.clear();
        self.file_filter.clear();
        for f in &result.files {
            self.selected_files.insert(f.path.clone());
        }
        self.current_match = if !result.files.is_empty() && !result.files[0].matches.is_empty() {
            let idx = result.files[0]
                .matches
                .iter()
                .position(|m| m.is_match)
                .unwrap_or(0);
            Some((0, idx))
        } else {
            None
        };
        self.scroll_to_current = self.current_match.is_some();
        self.status_msg = format!(
            "History: {} matches in {} files",
            result.total_matches,
            result.file_count()
        );
        self.update_active_tab(result);
    }

    fn rerun_history_entry(&mut self, result: &SearchResult) {
        self.ensure_empty_tab();
        self.params = result.params.clone();
        self.start_search();
    }

    fn do_replace_preview(&mut self) {
        let Some(result) = &self.current_result else {
            return;
        };
        let files_to_preview: Vec<crate::models::FileMatch> = match self.params.replace_scope {
            crate::models::ReplaceScope::Selected => result
                .files
                .iter()
                .filter(|f| self.selected_files.contains(&f.path))
                .cloned()
                .collect(),
            crate::models::ReplaceScope::All => {
                const MAX_PREVIEW_FILES: usize = 20;
                result
                    .files
                    .iter()
                    .take(MAX_PREVIEW_FILES)
                    .cloned()
                    .collect()
            }
        };
        if files_to_preview.is_empty() {
            return;
        }

        let regex = match build_regex(&self.params) {
            Ok(r) => r,
            Err(e) => {
                self.status_msg = format!("Regex error: {}", e);
                return;
            }
        };
        let params = self.params.clone();
        let mut entries: Vec<(PathBuf, String, String)> = Vec::new();
        for fm in &files_to_preview {
            let original = match std::fs::read_to_string(&fm.path) {
                Ok(s) => s,
                Err(e) => {
                    self.status_msg = format!("Read error: {}", e);
                    return;
                }
            };
            match apply_replace(fm, &regex, &params.replace_text) {
                Ok(preview) => entries.push((fm.path.clone(), original, preview)),
                Err(e) => {
                    self.status_msg = format!("Preview error: {}", e);
                    return;
                }
            }
        }
        self.replace_preview = Some((params, entries));
    }

    fn get_files_to_replace(&self) -> Vec<FileMatch> {
        let Some(result) = &self.current_result else {
            return Vec::new();
        };
        match self.params.replace_scope {
            crate::models::ReplaceScope::Selected => result
                .files
                .iter()
                .filter(|f| self.selected_files.contains(&f.path))
                .cloned()
                .collect(),
            crate::models::ReplaceScope::All => result.files.clone(),
        }
    }

    fn do_replace_all(&mut self) {
        let files = self.get_files_to_replace();
        if files.is_empty() {
            return;
        }
        self.replace_confirm_files = files.len();
        self.replace_confirm_matches = crate::grep::count_match_instances(&files);
        self.replace_confirm_snapshot = Some(files);
        self.show_replace_confirm = true;
    }

    fn execute_replace(&mut self) {
        let Some(files) = self.replace_confirm_snapshot.clone() else {
            self.show_replace_confirm = false;
            return;
        };
        let params = self.params.clone();
        let config = self.config.clone();

        if let Ok(regex) = build_regex(&params) {
            let mut ok = 0usize;
            let mut err = 0usize;
            let mut replaced_instances = 0usize;
            let session_dir_name = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
            let backup_root = std::path::Path::new(&config.backup_dir);

            for fm in &files {
                let match_count = fm
                    .matches
                    .iter()
                    .filter(|m| m.is_match)
                    .map(|m| m.ranges.len())
                    .sum::<usize>();

                if config.backup_before_replace {
                    if let Err(e) =
                        crate::grep::backup_file_to(&fm.path, backup_root, &session_dir_name)
                    {
                        self.status_msg = format!("Backup failed for {}: {}", fm.path.display(), e);
                        err += 1;
                        continue;
                    }
                }

                match apply_replace(fm, &regex, &params.replace_text) {
                    Ok(new_content) => {
                        if std::fs::write(&fm.path, new_content).is_ok() {
                            ok += 1;
                            replaced_instances += match_count;
                        } else {
                            err += 1;
                        }
                    }
                    Err(_) => err += 1,
                }
            }
            self.status_msg = format!(
                "Replaced {} instances in {} files ({} errors)",
                replaced_instances, ok, err
            );
        }
        self.replace_confirm_snapshot = None;
        self.show_replace_confirm = false;
    }

    fn copy_text(&mut self, ctx: &egui::Context, text: String) {
        ctx.copy_text(text);
        self.copied_flash = Some(std::time::Instant::now());
    }

    fn format_matches_to_string(
        &self,
        files: &[&FileMatch],
        params: &crate::models::SearchParams,
    ) -> String {
        format_matches_to_string_impl(&self.config, files, params)
    }
}

fn format_global_header(config: &Config, params: &crate::models::SearchParams) -> String {
    if !config.export_header_enabled {
        return String::new();
    }
    let format_str = config.export_header_format.as_str();
    if format_str.is_empty() {
        return String::new();
    }

    let mut result = String::new();
    let mut chars = format_str.chars().peekable();
    let now_str = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.peek() {
                Some('%') => {
                    chars.next();
                    result.push('%');
                }
                Some('q') => {
                    chars.next();
                    result.push_str(&params.pattern);
                }
                Some('d') => {
                    chars.next();
                    result.push_str(&params.directory);
                }
                Some('g') => {
                    chars.next();
                    result.push_str(&params.file_glob);
                }
                Some('x') => {
                    chars.next();
                    result.push_str(&params.exclude_glob);
                }
                Some('c') => {
                    chars.next();
                    result.push_str(&params.case_sensitive.to_string());
                }
                Some('r') => {
                    chars.next();
                    result.push_str(&params.is_regex.to_string());
                }
                Some('w') => {
                    chars.next();
                    result.push_str(&params.word_boundary.to_string());
                }
                Some('t') => {
                    chars.next();
                    result.push_str(&now_str);
                }
                Some('N') => {
                    chars.next();
                    result.push('\n');
                }
                Some('T') => {
                    chars.next();
                    result.push('\t');
                }
                _ => {
                    result.push('%');
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn format_matches_to_string_impl(
    config: &Config,
    files: &[&FileMatch],
    params: &crate::models::SearchParams,
) -> String {
    use crate::config::ExportOutputMode;

    let is_single_file = files.len() <= 1;
    let omit_filename = config.export_omit_single_file_name && is_single_file;

    let mut lines = Vec::new();

    let global_header = format_global_header(config, params);
    if !global_header.is_empty() {
        lines.push(global_header);
    }

    let mut line_format = config.export_line_format.as_str();
    if line_format.trim().is_empty() {
        line_format = "%f:%n:%l";
    }
    let mut file_header_format = config.export_file_header_format.as_str();
    if file_header_format.trim().is_empty() {
        file_header_format = "%f";
    }

    let pattern = &params.pattern;
    let directory = &params.directory;

    match config.export_output_mode {
        ExportOutputMode::Flat => {
            for fm in files {
                for lm in &fm.matches {
                    if !lm.is_match {
                        continue;
                    }
                    let mut fmt_str = line_format.to_string();
                    if omit_filename {
                        if fmt_str.contains("%f:") {
                            fmt_str = fmt_str.replace("%f:", "");
                        } else {
                            fmt_str = fmt_str.replace("%f", "");
                        }
                    }

                    let line_str = format_match_line(
                        pattern,
                        directory,
                        &fm.path,
                        lm.line_number,
                        &lm.content,
                        &lm.ranges,
                        &fmt_str,
                    );
                    lines.push(line_str);
                }
            }
        }
        ExportOutputMode::Grouped => {
            for fm in files {
                // Header
                if !omit_filename {
                    let header_str = format_file_header(directory, &fm.path, file_header_format);
                    if !header_str.is_empty() {
                        lines.push(header_str);
                    }
                }

                // Body lines
                for lm in &fm.matches {
                    if !lm.is_match {
                        continue;
                    }
                    let line_str = format_match_line(
                        pattern,
                        directory,
                        &fm.path,
                        lm.line_number,
                        &lm.content,
                        &lm.ranges,
                        line_format,
                    );
                    lines.push(line_str);
                }

                // Add an extra blank line between groups in grouped mode
                if files.len() > 1 {
                    lines.push(String::new());
                }
            }

            // Remove trailing blank line if we added one at the end
            if files.len() > 1 {
                lines.pop();
            }
        }
    }

    // Auto convert line endings to OS specific
    let delimiter = if cfg!(target_os = "windows") {
        "\r\n"
    } else {
        "\n"
    };

    lines.join(delimiter)
}

fn format_match_line(
    pattern: &str,
    search_root: &str,
    file_path: &std::path::Path,
    line_number: usize,
    line_content: &str,
    ranges: &[MatchRange],
    format_str: &str,
) -> String {
    // 1. Determine relative path of file
    let relative_path = file_path
        .strip_prefix(search_root)
        .unwrap_or(file_path)
        .to_string_lossy()
        .to_string();

    // 2. Extract matched keywords
    let matched_text = if !ranges.is_empty() {
        ranges
            .iter()
            .map(|r| {
                if r.start <= r.end && r.end <= line_content.len() {
                    &line_content[r.start..r.end]
                } else {
                    ""
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        pattern.to_string()
    };

    // 3. Perform placeholders replacement
    let mut result = String::new();
    let mut chars = format_str.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.peek() {
                Some('%') => {
                    chars.next();
                    result.push('%');
                }
                Some('f') => {
                    chars.next();
                    result.push_str(&relative_path);
                }
                Some('n') => {
                    chars.next();
                    result.push_str(&line_number.to_string());
                }
                Some('l') => {
                    chars.next();
                    result.push_str(line_content);
                }
                Some('m') => {
                    chars.next();
                    result.push_str(&matched_text);
                }
                Some('N') => {
                    chars.next();
                    result.push('\n');
                }
                Some('T') => {
                    chars.next();
                    result.push('\t');
                }
                _ => {
                    result.push('%');
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn format_file_header(search_root: &str, file_path: &std::path::Path, format_str: &str) -> String {
    let relative_path = file_path
        .strip_prefix(search_root)
        .unwrap_or(file_path)
        .to_string_lossy()
        .to_string();

    let mut result = String::new();
    let mut chars = format_str.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.peek() {
                Some('%') => {
                    chars.next();
                    result.push('%');
                }
                Some('f') => {
                    chars.next();
                    result.push_str(&relative_path);
                }
                Some('N') => {
                    chars.next();
                    result.push('\n');
                }
                Some('T') => {
                    chars.next();
                    result.push('\t');
                }
                _ => {
                    result.push('%');
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

// ── eframe::App ───────────────────────────────────────────────────────────────
impl eframe::App for GrepApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_search();
        self.ensure_theme_applied(&ctx);

        // Disable global shortcuts when any modal/subwindow is open
        let modal_open =
            self.show_replace_confirm || self.show_shortcuts || self.replace_preview.is_some();
        let enabled = !self.show_replace_confirm;

        // ── Global keyboard shortcuts ──────────────────────────────────────────
        let mut next_match_req = false;
        let mut prev_match_req = false;
        let mut next_file_req = false;
        let mut prev_file_req = false;
        let mut enter_req = false;
        let mut open_palette = false;
        let mut close_palette = false;

        ctx.input(|i| {
            let cmd = i.modifiers.command;
            let shift = i.modifiers.shift;

            // Ctrl/Cmd+K → open command palette (blocked when modal is open)
            if enabled && !modal_open && cmd && i.key_pressed(egui::Key::K) {
                open_palette = true;
            }
            // Ctrl/Cmd+F → focus pattern field (blocked when modal is open)
            if enabled && !modal_open && cmd && i.key_pressed(egui::Key::F) {
                self.focus_pattern = true;
            }
            // Ctrl/Cmd+T → new tab
            if enabled && !modal_open && cmd && i.key_pressed(egui::Key::T) {
                self.new_empty_tab();
            }
            // Escape → close overlays (palette first, then others)
            if i.key_pressed(egui::Key::Escape) {
                if self.show_palette {
                    close_palette = true;
                } else {
                    self.show_history = false;
                    self.show_shortcuts = false;
                    self.show_replace_confirm = false;
                    self.replace_confirm_snapshot = None;
                    if self.replace_preview.is_some() {
                        self.replace_preview = None;
                    } else if self.is_settings_active() {
                        self.close_settings_tab();
                    }
                }
            }

            if enabled && !self.show_palette {
                // F3 / Ctrl+G navigation
                if i.key_pressed(egui::Key::F3) || (cmd && i.key_pressed(egui::Key::G)) {
                    if shift {
                        prev_match_req = true;
                    } else {
                        next_match_req = true;
                    }
                }

                // File list arrow navigation
                if self.focused_pane == FocusedPane::FileList {
                    if i.key_pressed(egui::Key::ArrowDown) {
                        next_file_req = true;
                    }
                    if i.key_pressed(egui::Key::ArrowUp) {
                        prev_file_req = true;
                    }
                    if i.key_pressed(egui::Key::Enter) {
                        enter_req = true;
                    }
                }
            }
        });

        if open_palette {
            self.show_palette = true;
            self.palette_query.clear();
            self.palette_selected = 0;
            self.palette_focus = true;
            self.palette_instance = self.palette_instance.wrapping_add(1);
        }
        if close_palette {
            self.show_palette = false;
        }

        if enabled {
            if next_match_req {
                self.move_match_next();
            }
            if prev_match_req {
                self.move_match_prev();
            }
            if next_file_req {
                self.move_file_next();
            }
            if prev_file_req {
                self.move_file_prev();
            }
            if enter_req {
                self.open_selected_file_in_editor();
            }
        }

        let is_searching = matches!(*self.search_state.lock().unwrap(), SearchState::Running);
        if is_searching {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
        // Expire copy flash after 1.5s
        if let Some(t) = self.copied_flash {
            if t.elapsed().as_millis() > 1500 {
                self.copied_flash = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
        if let Some((_, t)) = self.copied_file_flash {
            if t.elapsed().as_millis() > 1500 {
                self.copied_file_flash = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }

        // ── OS folder drag-and-drop onto the window ────────────────────────────
        ctx.input(|i| {
            if let Some(dropped) = i.raw.dropped_files.first() {
                let path = dropped
                    .path
                    .as_deref()
                    .map(|p| p.to_string_lossy().into_owned());
                if let Some(p) = path {
                    self.params.directory = p;
                }
            }
        });

        let pal = self.pal;

        // ── Tab strip ─────────────────────────────────────────────────────────
        // Pre-collect tab metadata to avoid long borrows inside the panel closure
        let tabs_info: Vec<(String, String, bool, bool)> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let is_active = self.active_tab == Some(i);
                let (label, tooltip) = if tab.is_settings {
                    ("Settings".to_string(), "Settings".to_string())
                } else if let Some(result) = &tab.result {
                    let pat = &result.params.pattern;
                    let dir_tail = result
                        .params
                        .directory
                        .split(['/', '\\'])
                        .rfind(|s| !s.is_empty())
                        .unwrap_or("")
                        .to_string();
                    let pat_short = if pat.chars().count() > 16 {
                        format!("{}…", pat.chars().take(16).collect::<String>())
                    } else {
                        pat.clone()
                    };
                    let lbl = if dir_tail.is_empty() {
                        format!("\"{}\"", pat_short)
                    } else {
                        format!("\"{}\" · {}", pat_short, dir_tail)
                    };
                    let tip = format!(
                        "{}\nin {}\n{} matches",
                        pat, result.params.directory, result.total_matches
                    );
                    (lbl, tip)
                } else {
                    ("new".to_string(), "Empty tab".to_string())
                };
                (label, tooltip, is_active, tab.is_settings)
            })
            .collect();

        let mut tab_switch: Option<usize> = None;
        let mut tab_close: Option<usize> = None;
        let mut add_new_tab = false;

        // History panel at global level so it spans full window height (outside tabs)
        if self.show_history {
            egui::Panel::right("history_panel")
                .default_size(300.0)
                .min_size(220.0)
                .frame(egui::Frame::NONE.fill(pal.bg_mantle))
                .show_inside(ui, |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        self.show_history_panel(ui);
                    });
                });
        }

        egui::Panel::top("tab_strip")
            .frame(egui::Frame::NONE.fill(pal.bg_mantle).inner_margin(Margin {
                left: 4,
                right: 4,
                top: 3,
                bottom: 0,
            }))
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    // Right side: fixed Settings + History icons
                    let icon_reserve = 72.0;
                    let avail_h = ui.available_height();
                    let tab_w = (ui.available_width() - icon_reserve).max(60.0);

                    // Left side: scrollable tab area
                    ui.allocate_ui(Vec2::new(tab_w, avail_h), |ui| {
                        ScrollArea::horizontal()
                            .id_salt("tab_strip_scroll")
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(1.0, 0.0);
                                    for (i, (label, tooltip, is_active, is_settings_tab)) in
                                        tabs_info.iter().enumerate()
                                    {
                                        let bg = if *is_active {
                                            pal.bg_surface0
                                        } else {
                                            pal.bg_mantle
                                        };
                                        let text_color =
                                            if *is_active { pal.accent } else { pal.subtext };
                                        let rounding = egui::CornerRadius {
                                            nw: 4,
                                            ne: 4,
                                            sw: 0,
                                            se: 0,
                                        };
                                        let frame_resp = egui::Frame::NONE
                                            .fill(bg)
                                            .corner_radius(rounding)
                                            .inner_margin(Margin {
                                                left: 8,
                                                right: 6,
                                                top: 3,
                                                bottom: 3,
                                            })
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.spacing_mut().item_spacing =
                                                        Vec2::new(4.0, 0.0);
                                                    if *is_settings_tab {
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new(icons::SETTINGS)
                                                                    .color(text_color)
                                                                    .size(12.0)
                                                                    .family(
                                                                        egui::FontFamily::Name(
                                                                            "Icons".into(),
                                                                        ),
                                                                    ),
                                                            )
                                                            .sense(egui::Sense::hover()),
                                                        );
                                                    }
                                                    let lbl = ui.add(
                                                        egui::Label::new(
                                                            RichText::new(label)
                                                                .color(text_color)
                                                                .size(11.5),
                                                        )
                                                        .sense(egui::Sense::click()),
                                                    );
                                                    if lbl.clicked() && !is_active {
                                                        tab_switch = Some(i);
                                                    }
                                                    let close = ui.add(
                                                        egui::Label::new(
                                                            RichText::new("×")
                                                                .color(pal.muted)
                                                                .size(11.0),
                                                        )
                                                        .sense(egui::Sense::click()),
                                                    );
                                                    if close.clicked() {
                                                        tab_close = Some(i);
                                                    }
                                                });
                                            });
                                        frame_resp
                                            .response
                                            .on_hover_text(tooltip)
                                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                                    }
                                    ui.add_space(4.0);
                                    if ui
                                        .add(
                                            egui::Button::new(icon_rt(
                                                icons::ADD,
                                                13.0,
                                                pal.subtext,
                                            ))
                                            .frame(false),
                                        )
                                        .on_hover_text("New tab")
                                        .clicked()
                                    {
                                        add_new_tab = true;
                                    }
                                });
                            });
                    });

                    // Fixed icon cluster: Settings + History (always at right of tab bar)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                        self.icon_toggle(ui, "show_settings", icons::SETTINGS, "Settings");
                        self.icon_toggle(ui, "show_history", icons::HISTORY, "History");
                    });
                });
            });
        if let Some(i) = tab_close {
            self.close_tab(i);
        } else if let Some(i) = tab_switch {
            self.switch_to_tab(i);
        } else if add_new_tab {
            self.new_empty_tab();
        }

        let settings_active = self.is_settings_active();

        if !settings_active {
            egui::Panel::top("toolbar")
                .frame(toolbar_frame(pal))
                .show_inside(ui, |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.add_space(5.0);
                        self.show_toolbar(ui, is_searching);
                        ui.add_space(3.0);
                    });
                });

            if self.show_replace {
                egui::Panel::top("replace_bar")
                    .frame(toolbar_frame(pal))
                    .show_inside(ui, |ui| {
                        ui.add_enabled_ui(enabled, |ui| {
                            ui.add_space(3.0);
                            self.show_replace_bar(ui);
                            ui.add_space(3.0);
                        });
                    });
            }

            if is_searching {
                egui::Panel::top("progress_bar")
                    .exact_size(3.0)
                    .frame(egui::Frame::NONE.fill(pal.bg_mantle))
                    .show_inside(ui, |ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
                        let painter = ui.painter_at(rect);
                        let scanned = self
                            .search_scanned
                            .load(std::sync::atomic::Ordering::Relaxed);
                        let total = self.search_total.load(std::sync::atomic::Ordering::Relaxed);
                        if total > 0 {
                            let fraction = (scanned as f32 / total as f32).min(1.0);
                            let fill = egui::Rect::from_min_size(
                                rect.min,
                                egui::Vec2::new(rect.width() * fraction, rect.height()),
                            );
                            painter.rect_filled(fill, 0.0, pal.accent);
                        } else {
                            let t = ui.input(|i| i.time) as f32;
                            let stripe_w = rect.width() * 0.3;
                            let phase = (t * 0.7) % 1.3 - 0.15;
                            let x0 = (rect.min.x + phase * rect.width()).max(rect.min.x);
                            let x1 = (x0 + stripe_w).min(rect.max.x);
                            if x1 > x0 {
                                let fill =
                                    egui::Rect::from_x_y_ranges(x0..=x1, rect.min.y..=rect.max.y);
                                painter.rect_filled(fill, 0.0, pal.accent);
                            }
                        }
                        ui.ctx().request_repaint();
                    });
            }
        }

        // Status bar at very bottom
        egui::Panel::bottom("status_bar")
            .frame(egui::Frame::NONE.fill(pal.bg_mantle).inner_margin(Margin {
                left: 12,
                right: 12,
                top: 3,
                bottom: 3,
            }))
            .show_inside(ui, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    ui.horizontal(|ui| {
                        if is_searching {
                            ui.spinner();
                            ui.add_space(4.0);
                        }
                        // Copied flash
                        if self.copied_flash.is_some() {
                            ui.label(
                                RichText::new("Copied to clipboard")
                                    .color(pal.green)
                                    .size(12.0),
                            );
                            ui.separator();
                        }
                        let status = if is_searching {
                            let s = self
                                .search_scanned
                                .load(std::sync::atomic::Ordering::Relaxed);
                            let h = self.search_hits.load(std::sync::atomic::Ordering::Relaxed);
                            if s > 0 {
                                format!("Searching…  {} files  /  {} hits", s, h)
                            } else {
                                self.status_msg.clone()
                            }
                        } else {
                            self.status_msg.clone()
                        };
                        ui.label(RichText::new(&status).color(pal.subtext).size(12.0));
                    });
                });
            });

        if !settings_active {
            egui::Panel::left("file_list")
                .default_size(260.0)
                .min_size(180.0)
                .frame(egui::Frame::NONE.fill(pal.bg_mantle))
                .show_inside(ui, |ui| {
                    // Prevent content from pushing the panel wider than its allocated width.
                    ui.set_max_width(ui.available_width());
                    if enabled && ui.rect_contains_pointer(ui.max_rect()) {
                        self.focused_pane = FocusedPane::FileList;
                    }
                    ui.add_enabled_ui(enabled, |ui| {
                        self.show_file_panel(ui);
                    });
                });
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(pal.bg_base))
            .show_inside(ui, |ui| {
                if settings_active {
                    ui.add_enabled_ui(enabled, |ui| {
                        self.show_settings_panel(ui);
                    });
                } else {
                    if enabled && ui.rect_contains_pointer(ui.max_rect()) {
                        self.focused_pane = FocusedPane::Content;
                    }
                    ui.add_enabled_ui(enabled, |ui| {
                        self.show_content_panel(ui);
                    });
                }
            });

        let screen_size = ctx.content_rect().size();
        let max_w = screen_size.x * 0.95;
        let max_h = screen_size.y * 0.95;

        if self.replace_preview.is_some() {
            egui::Window::new("Replace Preview")
                .collapsible(false)
                .resizable(true)
                .vscroll(true)
                .hscroll(true)
                .default_size([640.0, 460.0])
                .max_width(max_w)
                .max_height(max_h)
                .show(&ctx, |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        self.show_replace_preview_window(ui);
                    });
                });
        }

        if self.show_replace_confirm {
            egui::Window::new("Confirm Replacement")
                .collapsible(false)
                .resizable(true)
                .vscroll(true)
                .hscroll(true)
                .max_width(max_w)
                .max_height(max_h)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(&ctx, |ui| {
                    self.show_replace_confirm_window(ui);
                });
        }

        if self.show_shortcuts {
            let mut open = self.show_shortcuts;
            egui::Window::new("Keyboard Shortcuts")
                .collapsible(false)
                .resizable(true)
                .vscroll(true)
                .hscroll(true)
                .default_size([360.0, 400.0])
                .max_width(max_w)
                .max_height(max_h)
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(&ctx, |ui| {
                    self.show_shortcuts_window(ui);
                });
            self.show_shortcuts = open;
        }

        let palette_dur = if self.config.reduce_motion { 0.0 } else { 0.12 };
        let palette_anim = ctx.animate_bool_with_time(
            egui::Id::new("palette_anim"),
            self.show_palette,
            palette_dur,
        );
        if palette_anim > 0.001 {
            self.show_command_palette(&ctx, palette_anim);
        }
    }
}

// ── Command Palette ───────────────────────────────────────────────────────────
impl GrepApp {
    fn palette_items(&self) -> Vec<(String, String, PaletteAction)> {
        let mut items: Vec<(String, String, PaletteAction)> = vec![
            (
                "Focus search pattern".into(),
                "Ctrl+F".into(),
                PaletteAction::FocusPattern,
            ),
            ("Focus directory".into(), "".into(), PaletteAction::FocusDir),
            ("New tab".into(), "Ctrl+T".into(), PaletteAction::NewTab),
            (
                "Toggle history".into(),
                "History".into(),
                PaletteAction::ToggleHistory,
            ),
            (
                "Toggle settings".into(),
                "Settings".into(),
                PaletteAction::ToggleSettings,
            ),
            (
                "Toggle replace bar".into(),
                "Replace".into(),
                PaletteAction::ToggleReplace,
            ),
            (
                "Theme: System Default".into(),
                "Follow OS settings".into(),
                PaletteAction::SwitchTheme(Theme::System),
            ),
            (
                "Theme: Dark".into(),
                "Catppuccin Mocha".into(),
                PaletteAction::SwitchTheme(Theme::Dark),
            ),
            (
                "Theme: Light".into(),
                "Catppuccin Latte".into(),
                PaletteAction::SwitchTheme(Theme::Light),
            ),
            (
                "Theme: High Contrast".into(),
                "".into(),
                PaletteAction::SwitchTheme(Theme::HighContrast),
            ),
            (
                "Re-run last search".into(),
                "".into(),
                PaletteAction::RerunSearch,
            ),
            (
                "Clear results".into(),
                "".into(),
                PaletteAction::ClearResults,
            ),
            (
                "Keyboard shortcuts".into(),
                "?".into(),
                PaletteAction::ShowShortcuts,
            ),
        ];
        for dir in self.recent_dirs() {
            items.push((
                format!("Dir: {}", dir),
                "recent".into(),
                PaletteAction::SetDir(dir),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for entry in self.history.entries.iter().rev().take(5) {
            let p = &entry.params.pattern;
            if !p.is_empty() && seen.insert(p.clone()) {
                items.push((
                    format!("Pattern: {}", p),
                    "recent".into(),
                    PaletteAction::SetPattern(p.clone()),
                ));
            }
        }
        items
    }

    fn execute_palette_action(&mut self, action: PaletteAction) {
        match action {
            PaletteAction::FocusPattern => {
                self.focus_pattern = true;
            }
            PaletteAction::FocusDir => {
                self.focus_dir = true;
            }
            PaletteAction::NewTab => {
                self.new_empty_tab();
            }
            PaletteAction::ToggleHistory => {
                self.show_history = !self.show_history;
            }
            PaletteAction::ToggleSettings => {
                self.open_settings_tab();
            }
            PaletteAction::ToggleReplace => {
                self.show_replace = !self.show_replace;
            }
            PaletteAction::SwitchTheme(t) => {
                self.config.theme = t;
                let _ = self.config.save();
            }
            PaletteAction::RerunSearch => {
                if self.current_result.is_some() {
                    self.start_search();
                }
            }
            PaletteAction::ClearResults => {
                if let Some(idx) = self.active_tab {
                    self.close_tab(idx);
                }
            }
            PaletteAction::SetDir(d) => {
                self.params.directory = d;
            }
            PaletteAction::SetPattern(p) => {
                self.params.pattern = p;
                self.focus_pattern = true;
            }
            PaletteAction::ShowShortcuts => {
                self.show_shortcuts = true;
            }
        }
    }

    fn show_command_palette(&mut self, ctx: &egui::Context, anim: f32) {
        let pal = self.pal;
        let q = self.palette_query.to_lowercase();
        let all_items = self.palette_items();
        let filtered: Vec<_> = if q.is_empty() {
            all_items.clone()
        } else {
            all_items
                .into_iter()
                .filter(|(label, _, _)| label.to_lowercase().contains(&q))
                .collect()
        };
        let n = filtered.len();
        if self.palette_selected >= n && n > 0 {
            self.palette_selected = n - 1;
        }

        // Navigate via ↑/↓ and execute via Enter — only when palette is open (not fading out)
        let mut nav_delta: i32 = 0;
        let mut execute_idx: Option<usize> = None;
        let mut close = false;
        if self.show_palette {
            ctx.input(|i| {
                if i.key_pressed(egui::Key::ArrowDown) {
                    nav_delta = 1;
                }
                if i.key_pressed(egui::Key::ArrowUp) {
                    nav_delta = -1;
                }
                if i.key_pressed(egui::Key::Enter) && n > 0 {
                    execute_idx = Some(self.palette_selected);
                }
                if i.key_pressed(egui::Key::Escape) {
                    close = true;
                }
            });
            if close {
                self.show_palette = false;
            }
            if nav_delta != 0 && n > 0 {
                let sel = self.palette_selected as i32 + nav_delta;
                self.palette_selected = sel.rem_euclid(n as i32) as usize;
            }
        }

        let palette_id = egui::Id::new("command_palette_query");

        // Dim background (alpha fades in/out with anim)
        let dim_alpha = (anim * 120.0) as u8;
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("palette_dim"),
        ))
        .rect_filled(
            ctx.content_rect(),
            0.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, dim_alpha),
        );

        let center = ctx.content_rect().center();
        let win_w = 520.0_f32;
        let item_h = 36.0_f32;
        let max_visible = 8usize;
        let list_h = (n.min(max_visible) as f32) * item_h;

        egui::Area::new(egui::Id::new("command_palette_area").with(self.palette_instance))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(
                center.x - win_w / 2.0,
                center.y - (list_h + 48.0) / 2.0,
            ))
            .show(ctx, |ui| {
                let frame_a = (anim * 255.0) as u8;
                let frame_fill = Color32::from_rgba_unmultiplied(
                    pal.bg_surface0.r(),
                    pal.bg_surface0.g(),
                    pal.bg_surface0.b(),
                    frame_a,
                );
                let border_a = (anim * 255.0) as u8;
                let border_col = Color32::from_rgba_unmultiplied(
                    pal.bg_surface1.r(),
                    pal.bg_surface1.g(),
                    pal.bg_surface1.b(),
                    border_a,
                );
                egui::Frame::NONE
                    .fill(frame_fill)
                    .corner_radius(egui::CornerRadius::same(8))
                    .stroke(egui::Stroke::new(1.0, border_col))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(win_w - 16.0);

                        // Query input
                        let query_resp = ui.add(
                            egui::TextEdit::singleline(&mut self.palette_query)
                                .id(palette_id)
                                .hint_text(
                                    RichText::new("Type a command…")
                                        .color(pal.placeholder)
                                        .italics(),
                                )
                                .desired_width(f32::INFINITY)
                                .font(egui::TextStyle::Body),
                        );
                        if self.palette_focus {
                            query_resp.request_focus();
                            self.palette_focus = false;
                        }

                        if n == 0 {
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("No commands match")
                                    .color(pal.muted)
                                    .size(12.0),
                            );
                            return;
                        }

                        ui.add_space(4.0);

                        ScrollArea::vertical()
                            .id_salt(self.palette_instance)
                            .max_height(list_h)
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = Vec2::ZERO;
                                for (idx, (label, hint, _action)) in filtered.iter().enumerate() {
                                    let selected = idx == self.palette_selected;
                                    let bg = if selected {
                                        pal.bg_surface1
                                    } else {
                                        pal.bg_surface0
                                    };
                                    let text_color = if selected { pal.text } else { pal.subtext };

                                    let row = egui::Frame::NONE
                                        .fill(bg)
                                        .corner_radius(egui::CornerRadius::same(4))
                                        .inner_margin(egui::Margin {
                                            left: 8,
                                            right: 8,
                                            top: 4,
                                            bottom: 4,
                                        })
                                        .show(ui, |ui| {
                                            ui.set_min_width(win_w - 32.0);
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    RichText::new(label)
                                                        .color(text_color)
                                                        .size(13.0),
                                                );
                                                if !hint.is_empty() {
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.label(
                                                                RichText::new(hint)
                                                                    .color(pal.muted)
                                                                    .size(11.0),
                                                            );
                                                        },
                                                    );
                                                }
                                            });
                                        });

                                    let row_resp = ui.interact(
                                        row.response.rect,
                                        egui::Id::new(("palette_item", idx)),
                                        egui::Sense::click(),
                                    );
                                    if row_resp.hovered() {
                                        self.palette_selected = idx;
                                    }
                                    if row_resp.clicked() {
                                        execute_idx = Some(idx);
                                    }
                                }
                            });
                    });
            });

        if let Some(idx) = execute_idx {
            if let Some((_, _, action)) = filtered.get(idx) {
                let action = action.clone();
                self.show_palette = false;
                self.execute_palette_action(action);
            }
        }
    }
}

// ── Toolbar ───────────────────────────────────────────────────────────────────
impl GrepApp {
    fn show_toolbar(&mut self, ui: &mut Ui, is_searching: bool) {
        let pal = self.pal;
        let hint = |text: &str| RichText::new(text).color(pal.placeholder).italics();
        let available_w = ui.available_width();

        // IDs for explicit Tab-order navigation (Dir → Pattern → Inc → Exc → Dir)
        let dir_id = egui::Id::new("dir_input");
        let pat_id = egui::Id::new("pattern_input");
        let inc_id = egui::Id::new("inc_input");
        let exc_id = egui::Id::new("exc_input");

        let label_w = 40.0_f32;
        let row_h = ui.spacing().interact_size.y;
        let narrow = available_w < 900.0;

        // Shared input width so the Dir and Pattern fields are exactly the same size.
        // Reserve room for the widest trailing controls of either row (the Search button,
        // optionally plus the Replace toggle on narrow screens).
        let trailing_reserve = if narrow { 132.0 } else { 100.0 };
        let input_w = (available_w - label_w - 6.0 - trailing_reserve).max(120.0);

        let recent_dirs = self.recent_dirs();
        let recent_patterns = self.recent_patterns();
        let recent_includes = self.recent_includes();
        let recent_excludes = self.recent_excludes();
        let dir_popup_id = egui::Id::new("dir_history_popup");
        let pat_popup_id = egui::Id::new("pattern_history_popup");
        let inc_popup_id = egui::Id::new("inc_history_popup");
        let exc_popup_id = egui::Id::new("exc_history_popup");
        let mut picked_dir: Option<String> = None;
        let mut picked_pattern: Option<String> = None;
        let mut inc_resp_outer: Option<egui::Response> = None;
        let mut exc_resp_outer: Option<egui::Response> = None;
        let mut inc_filtered: Vec<String> = vec![];
        let mut exc_filtered: Vec<String> = vec![];
        let mut picked_inc: Option<String> = None;
        let mut picked_exc: Option<String> = None;

        // ── Row 1: Dir ────────────────────────────────────────────────
        let mut dir_resp_outer: Option<egui::Response> = None;
        let mut dir_filtered: Vec<String> = vec![];
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);

            ui.allocate_ui_with_layout(
                Vec2::new(label_w, row_h),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.label(RichText::new("Dir").color(pal.subtext).size(12.0));
                },
            );

            let dir_resp = ui.add(
                egui::TextEdit::singleline(&mut self.params.directory)
                    .id(dir_id)
                    .hint_text(hint("path/to/directory"))
                    .desired_width(input_w),
            );
            if self.focus_dir {
                dir_resp.request_focus();
                self.focus_dir = false;
            }

            // Compute filtered suggestions from current text
            let dq = self.params.directory.to_lowercase();
            dir_filtered = recent_dirs
                .iter()
                .filter(|d| dq.is_empty() || d.to_lowercase().contains(&dq))
                .take(8)
                .cloned()
                .collect();

            // Auto-open/close as text changes (only open when field has focus to
            // avoid reopening when backing string is changed externally after selection)
            if dir_resp.changed() {
                if !dir_filtered.is_empty() && dir_resp.has_focus() {
                    egui::Popup::open_id(ui.ctx(), dir_popup_id);
                } else if dir_filtered.is_empty() {
                    egui::Popup::close_id(ui.ctx(), dir_popup_id);
                }
                self.dir_suggest_idx = None;
            }
            // Auto-open on focus gain or re-click while already focused
            if (dir_resp.gained_focus() || dir_resp.clicked()) && !dir_filtered.is_empty() {
                egui::Popup::open_id(ui.ctx(), dir_popup_id);
                self.dir_suggest_idx = None;
            }

            let dir_popup_open = egui::Popup::is_id_open(ui.ctx(), dir_popup_id);
            let dir_n = dir_filtered.len();

            if dir_resp.has_focus() {
                if dir_popup_open {
                    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
                    {
                        self.dir_suggest_idx = Some(
                            self.dir_suggest_idx
                                .map_or(0, |i| (i + 1).min(dir_n.saturating_sub(1))),
                        );
                    }
                    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                        self.dir_suggest_idx =
                            Some(self.dir_suggest_idx.map_or(0, |i| i.saturating_sub(1)));
                    }
                    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                        egui::Popup::close_id(ui.ctx(), dir_popup_id);
                        self.dir_suggest_idx = None;
                    }
                } else if ui
                    .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
                    && !dir_filtered.is_empty()
                {
                    egui::Popup::open_id(ui.ctx(), dir_popup_id);
                    self.dir_suggest_idx = Some(0);
                }
                // Tab: Dir → Pattern  /  Shift+Tab: Dir → Exc (filters visible) or Pat (filters hidden)
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
                    ui.ctx().memory_mut(|m| m.request_focus(pat_id));
                    egui::Popup::close_id(ui.ctx(), dir_popup_id);
                    self.dir_suggest_idx = None;
                } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab)) {
                    if self.config.show_advanced {
                        ui.ctx().memory_mut(|m| m.request_focus(exc_id));
                    } else {
                        ui.ctx().memory_mut(|m| m.request_focus(pat_id));
                    }
                    egui::Popup::close_id(ui.ctx(), dir_popup_id);
                    self.dir_suggest_idx = None;
                }
            }
            // Enter: TextEdit.singleline surrenders focus on Enter, so handle via lost_focus
            if dir_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if dir_popup_open {
                    if let Some(idx) = self.dir_suggest_idx {
                        if idx < dir_n {
                            picked_dir = Some(dir_filtered[idx].clone());
                        }
                    } else {
                        self.start_search();
                    }
                    egui::Popup::close_id(ui.ctx(), dir_popup_id);
                    self.dir_suggest_idx = None;
                } else {
                    self.start_search();
                }
            }

            if ui
                .add(egui::Button::new(icon_rt(icons::FOLDER, 15.0, pal.accent)))
                .on_hover_text("Browse folder")
                .clicked()
            {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.params.directory = p.to_string_lossy().to_string();
                }
            }

            // Replace toggle on Dir row (wide screens only)
            if !narrow {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(2.0, 4.0);
                    self.icon_toggle(ui, "show_replace", icons::REPLACE, "Replace");
                });
            }

            dir_resp_outer = Some(dir_resp);
        });

        // Dir suggestion popup (rendered outside horizontal to avoid borrow issues)
        // dir_sel is hoisted so the post-popup lost_focus handler can see whether
        // a suggestion was clicked this frame (to avoid double-close vs. no-close race).
        let mut dir_sel: Option<String> = None;
        if !dir_filtered.is_empty() {
            if let Some(ref dr) = dir_resp_outer {
                let field_w = dr.rect.width();
                let cur_idx = self.dir_suggest_idx;
                let mut hov: Option<usize> = None;
                let popup_frame = egui::Frame::popup(ui.style())
                    .fill(pal.bg_surface0)
                    .corner_radius(egui::CornerRadius::same(6))
                    .stroke(egui::Stroke::new(1.0, pal.bg_surface1))
                    .inner_margin(Margin::same(4));
                egui::Popup::from_response(dr)
                    .id(dir_popup_id)
                    .open_memory(None)
                    .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
                    .frame(popup_frame)
                    .show(|ui| {
                        ui.set_min_width(field_w);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        for (i, sug) in dir_filtered.iter().enumerate() {
                            let selected = cur_idx == Some(i);
                            let bg = if selected {
                                pal.bg_surface1
                            } else {
                                egui::Color32::TRANSPARENT
                            };
                            let tc = if selected { pal.accent } else { pal.text };
                            let row = egui::Frame::NONE
                                .fill(bg)
                                .corner_radius(egui::CornerRadius::same(3))
                                .inner_margin(Margin {
                                    left: 8,
                                    right: 8,
                                    top: 4,
                                    bottom: 4,
                                })
                                .show(ui, |ui| {
                                    ui.set_min_width(field_w - 24.0);
                                    ui.add(
                                        egui::Label::new(RichText::new(sug).size(12.0).color(tc))
                                            .truncate(),
                                    )
                                });
                            let rr = ui.interact(
                                row.response.rect,
                                egui::Id::new("dir_sug").with(i as u32),
                                egui::Sense::click(),
                            );
                            if rr.hovered() {
                                hov = Some(i);
                            }
                            if rr.clicked() {
                                dir_sel = Some(sug.clone());
                            }
                        }
                    });
                if let Some(i) = hov {
                    self.dir_suggest_idx = Some(i);
                }
                if let Some(d) = dir_sel.clone() {
                    picked_dir = Some(d);
                    egui::Popup::close_id(ui.ctx(), dir_popup_id);
                    self.dir_suggest_idx = None;
                }
            }
        }
        // Close popup when the Dir field loses focus and no suggestion was selected.
        // This runs after the popup renders so same-frame click detection still works.
        if let Some(ref dr) = dir_resp_outer {
            if dr.lost_focus() && dir_sel.is_none() {
                egui::Popup::close_id(ui.ctx(), dir_popup_id);
                self.dir_suggest_idx = None;
            }
        }

        // ── Row 2: Pattern + Search ───────────────────────────────────
        ui.add_space(2.0);
        let mut pat_resp_outer: Option<egui::Response> = None;
        let mut pat_filtered: Vec<String> = vec![];
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
            ui.add_space(label_w + ui.spacing().item_spacing.x);

            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.params.pattern)
                    .id(pat_id)
                    .hint_text(hint("search pattern  (Ctrl+F)"))
                    .desired_width(input_w)
                    .font(egui::TextStyle::Monospace),
            );
            if self.focus_pattern {
                resp.request_focus();
                self.focus_pattern = false;
            }

            // Compute filtered suggestions from current text
            let pq = self.params.pattern.to_lowercase();
            pat_filtered = recent_patterns
                .iter()
                .filter(|p| pq.is_empty() || p.to_lowercase().contains(&pq))
                .take(8)
                .cloned()
                .collect();

            // Auto-open/close as text changes
            if resp.changed() {
                if !pat_filtered.is_empty() {
                    egui::Popup::open_id(ui.ctx(), pat_popup_id);
                } else {
                    egui::Popup::close_id(ui.ctx(), pat_popup_id);
                }
                self.pat_suggest_idx = None;
            }
            // Auto-open on focus gain or re-click while already focused
            // (suppressed when focus was restored after suggestion selection)
            if resp.gained_focus() || resp.clicked() {
                if self.pat_suppress_popup_open {
                    self.pat_suppress_popup_open = false;
                } else if !pat_filtered.is_empty() {
                    egui::Popup::open_id(ui.ctx(), pat_popup_id);
                    self.pat_suggest_idx = None;
                }
            }

            let pat_popup_open = egui::Popup::is_id_open(ui.ctx(), pat_popup_id);
            let pat_n = pat_filtered.len();

            if resp.has_focus() {
                if pat_popup_open {
                    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
                    {
                        self.pat_suggest_idx = Some(
                            self.pat_suggest_idx
                                .map_or(0, |i| (i + 1).min(pat_n.saturating_sub(1))),
                        );
                    }
                    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                        self.pat_suggest_idx =
                            Some(self.pat_suggest_idx.map_or(0, |i| i.saturating_sub(1)));
                    }
                    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                        egui::Popup::close_id(ui.ctx(), pat_popup_id);
                        self.pat_suggest_idx = None;
                    }
                } else if ui
                    .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
                    && !pat_filtered.is_empty()
                {
                    egui::Popup::open_id(ui.ctx(), pat_popup_id);
                    self.pat_suggest_idx = Some(0);
                }
                // Tab: Pattern → Inc (filters visible) or Dir (filters hidden)  /  Shift+Tab: Pattern → Dir
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
                    if self.config.show_advanced {
                        ui.ctx().memory_mut(|m| m.request_focus(inc_id));
                    } else {
                        ui.ctx().memory_mut(|m| m.request_focus(dir_id));
                    }
                    egui::Popup::close_id(ui.ctx(), pat_popup_id);
                    self.pat_suggest_idx = None;
                } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab)) {
                    ui.ctx().memory_mut(|m| m.request_focus(dir_id));
                    egui::Popup::close_id(ui.ctx(), pat_popup_id);
                    self.pat_suggest_idx = None;
                }
            }

            let btn_color = if is_searching { pal.red } else { pal.accent };
            let btn_text = if is_searching {
                "■  Stop"
            } else {
                "▶  Search"
            };
            if ui
                .add(
                    egui::Button::new(RichText::new(btn_text).color(pal.bg_mantle).size(13.0))
                        .fill(btn_color),
                )
                .clicked()
            {
                if is_searching {
                    self.cancel_search();
                } else {
                    self.start_search();
                }
            }

            // Narrow: Replace toggle moves to end of pattern row
            if narrow {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(2.0, 4.0);
                    self.icon_toggle(ui, "show_replace", icons::REPLACE, "Replace");
                });
            }

            // Enter: TextEdit.singleline surrenders focus on Enter, handle via lost_focus
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if pat_popup_open {
                    if let Some(idx) = self.pat_suggest_idx {
                        if idx < pat_n {
                            picked_pattern = Some(pat_filtered[idx].clone());
                        }
                    } else {
                        self.start_search();
                    }
                    egui::Popup::close_id(ui.ctx(), pat_popup_id);
                    self.pat_suggest_idx = None;
                } else {
                    self.start_search();
                }
            }

            pat_resp_outer = Some(resp);
        });

        // Pattern suggestion popup
        let mut pat_sel: Option<String> = None;
        if !pat_filtered.is_empty() {
            if let Some(ref pr) = pat_resp_outer {
                let field_w = pr.rect.width();
                let cur_idx = self.pat_suggest_idx;
                let mut hov: Option<usize> = None;
                let popup_frame = egui::Frame::popup(ui.style())
                    .fill(pal.bg_surface0)
                    .corner_radius(egui::CornerRadius::same(6))
                    .stroke(egui::Stroke::new(1.0, pal.bg_surface1))
                    .inner_margin(Margin::same(4));
                egui::Popup::from_response(pr)
                    .id(pat_popup_id)
                    .open_memory(None)
                    .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
                    .frame(popup_frame)
                    .show(|ui| {
                        ui.set_min_width(field_w);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        for (i, sug) in pat_filtered.iter().enumerate() {
                            let selected = cur_idx == Some(i);
                            let bg = if selected {
                                pal.bg_surface1
                            } else {
                                egui::Color32::TRANSPARENT
                            };
                            let tc = if selected { pal.accent } else { pal.text };
                            let row = egui::Frame::NONE
                                .fill(bg)
                                .corner_radius(egui::CornerRadius::same(3))
                                .inner_margin(Margin {
                                    left: 8,
                                    right: 8,
                                    top: 4,
                                    bottom: 4,
                                })
                                .show(ui, |ui| {
                                    ui.set_min_width(field_w - 24.0);
                                    ui.add(
                                        egui::Label::new(RichText::new(sug).size(12.0).color(tc))
                                            .truncate(),
                                    )
                                });
                            let rr = ui.interact(
                                row.response.rect,
                                egui::Id::new("pat_sug").with(i as u32),
                                egui::Sense::click(),
                            );
                            if rr.hovered() {
                                hov = Some(i);
                            }
                            if rr.clicked() {
                                pat_sel = Some(sug.clone());
                            }
                        }
                    });
                if let Some(i) = hov {
                    self.pat_suggest_idx = Some(i);
                }
                if let Some(p) = pat_sel.clone() {
                    picked_pattern = Some(p);
                    egui::Popup::close_id(ui.ctx(), pat_popup_id);
                    self.pat_suggest_idx = None;
                }
            }
        }
        // Close popup when the Pattern field loses focus and no suggestion was selected.
        if let Some(ref pr) = pat_resp_outer {
            if pr.lost_focus() && pat_sel.is_none() {
                egui::Popup::close_id(ui.ctx(), pat_popup_id);
                self.pat_suggest_idx = None;
            }
        }

        // ── Row 3: Filters (collapsible) ──────────────────────────────
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(2.0, 4.0);
            ui.add_space(label_w + 4.0);
            let chevron = if self.config.show_advanced {
                icons::CHEVRON_DOWN
            } else {
                icons::CHEVRON_RIGHT
            };
            let chev = ui.add(egui::Button::new(icon_rt(chevron, 13.0, pal.subtext)).frame(false));
            let lbl = ui
                .add(
                    egui::Label::new(RichText::new("Filters").color(pal.subtext).size(12.0))
                        .sense(egui::Sense::click()),
                )
                .on_hover_text(
                    "Include / Exclude / Regex / Case / Word / Context / Depth / Type / Extra dirs",
                );
            if lbl.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if chev.clicked() || lbl.clicked() {
                self.config.show_advanced = !self.config.show_advanced;
                let _ = self.config.save();
            }
        });
        if self.config.show_advanced {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                ui.add_space(label_w + ui.spacing().item_spacing.x);
                let (ir, ifl, er, efl) = show_filter_flags(
                    ui,
                    &mut self.params,
                    pal,
                    inc_id,
                    exc_id,
                    dir_id,
                    pat_id,
                    &recent_includes,
                    &recent_excludes,
                    inc_popup_id,
                    exc_popup_id,
                    &mut self.inc_suggest_idx,
                    &mut self.exc_suggest_idx,
                );
                inc_resp_outer = ir;
                inc_filtered = ifl;
                exc_resp_outer = er;
                exc_filtered = efl;
            });

            // Type presets
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                ui.add_space(label_w + ui.spacing().item_spacing.x);
                ui.label(RichText::new("Type:").color(pal.subtext).size(11.0));

                // Static "All" preset
                {
                    let active = self.params.file_glob.is_empty();
                    preset_chip(ui, pal, "All", active, "Clear type filter", || {
                        self.params.file_glob.clear();
                    });
                }

                // Dynamic presets from config
                for preset in &self.config.presets {
                    if !preset.enabled {
                        continue;
                    }
                    let active = self.params.file_glob == preset.glob;
                    let glob = preset.glob.clone();
                    let name = preset.name.clone();
                    let tooltip = format!("Filter: {glob}");
                    preset_chip(ui, pal, &name, active, &tooltip, || {
                        self.params.file_glob = glob.clone();
                    });
                }
            });

            // ── Additional search roots ────────────────────────────────
            let mut remove_root_idx: Option<usize> = None;
            for (i, root) in self.params.roots.iter_mut().enumerate() {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(label_w, row_h),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.label(RichText::new("+Dir").color(pal.subtext).size(12.0));
                        },
                    );
                    ui.add(
                        egui::TextEdit::singleline(root)
                            .hint_text(hint("additional path"))
                            .desired_width(input_w),
                    );
                    if ui
                        .add(egui::Button::new(icon_rt(icons::FOLDER, 15.0, pal.accent)))
                        .on_hover_text("Browse folder")
                        .clicked()
                    {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            *root = p.to_string_lossy().to_string();
                        }
                    }
                    if ui
                        .add(egui::Button::new(icon_rt(icons::CLOSE, 14.0, pal.red)).frame(false))
                        .on_hover_text("Remove this root")
                        .clicked()
                    {
                        remove_root_idx = Some(i);
                    }
                });
            }
            if let Some(i) = remove_root_idx {
                self.params.roots.remove(i);
            }
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                ui.add_space(label_w + ui.spacing().item_spacing.x);
                if ui
                    .add(egui::Button::new(icon_rt(icons::ADD, 13.0, pal.accent)).frame(false))
                    .on_hover_text("Add another search root")
                    .clicked()
                {
                    self.params.roots.push(String::new());
                }
            });
        }

        // Inc suggestion popup
        let mut inc_sel: Option<String> = None;
        if !inc_filtered.is_empty() {
            if let Some(ref ir) = inc_resp_outer {
                let field_w = ir.rect.width();
                let cur_idx = self.inc_suggest_idx;
                let mut hov: Option<usize> = None;
                let popup_frame = egui::Frame::popup(ui.style())
                    .fill(pal.bg_surface0)
                    .corner_radius(egui::CornerRadius::same(6))
                    .stroke(egui::Stroke::new(1.0, pal.bg_surface1))
                    .inner_margin(Margin::same(4));
                egui::Popup::from_response(ir)
                    .id(inc_popup_id)
                    .open_memory(None)
                    .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
                    .frame(popup_frame)
                    .show(|ui| {
                        ui.set_min_width(field_w);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        for (i, sug) in inc_filtered.iter().enumerate() {
                            let selected = cur_idx == Some(i);
                            let bg = if selected {
                                pal.bg_surface1
                            } else {
                                egui::Color32::TRANSPARENT
                            };
                            let tc = if selected { pal.accent } else { pal.text };
                            let row = egui::Frame::NONE
                                .fill(bg)
                                .corner_radius(egui::CornerRadius::same(3))
                                .inner_margin(Margin {
                                    left: 8,
                                    right: 8,
                                    top: 4,
                                    bottom: 4,
                                })
                                .show(ui, |ui| {
                                    ui.set_min_width(field_w - 24.0);
                                    ui.add(
                                        egui::Label::new(RichText::new(sug).size(12.0).color(tc))
                                            .truncate(),
                                    )
                                });
                            let rr = ui.interact(
                                row.response.rect,
                                egui::Id::new("inc_sug").with(i as u32),
                                egui::Sense::click(),
                            );
                            if rr.hovered() {
                                hov = Some(i);
                            }
                            if rr.clicked() {
                                inc_sel = Some(sug.clone());
                            }
                        }
                    });
                if let Some(i) = hov {
                    self.inc_suggest_idx = Some(i);
                }
                if let Some(v) = inc_sel.clone() {
                    picked_inc = Some(v);
                    egui::Popup::close_id(ui.ctx(), inc_popup_id);
                    self.inc_suggest_idx = None;
                }
            }
        }
        if let Some(ref ir) = inc_resp_outer {
            if ir.lost_focus() && inc_sel.is_none() {
                egui::Popup::close_id(ui.ctx(), inc_popup_id);
                self.inc_suggest_idx = None;
            }
        }

        // Exc suggestion popup
        let mut exc_sel: Option<String> = None;
        if !exc_filtered.is_empty() {
            if let Some(ref er) = exc_resp_outer {
                let field_w = er.rect.width();
                let cur_idx = self.exc_suggest_idx;
                let mut hov: Option<usize> = None;
                let popup_frame = egui::Frame::popup(ui.style())
                    .fill(pal.bg_surface0)
                    .corner_radius(egui::CornerRadius::same(6))
                    .stroke(egui::Stroke::new(1.0, pal.bg_surface1))
                    .inner_margin(Margin::same(4));
                egui::Popup::from_response(er)
                    .id(exc_popup_id)
                    .open_memory(None)
                    .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
                    .frame(popup_frame)
                    .show(|ui| {
                        ui.set_min_width(field_w);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        for (i, sug) in exc_filtered.iter().enumerate() {
                            let selected = cur_idx == Some(i);
                            let bg = if selected {
                                pal.bg_surface1
                            } else {
                                egui::Color32::TRANSPARENT
                            };
                            let tc = if selected { pal.accent } else { pal.text };
                            let row = egui::Frame::NONE
                                .fill(bg)
                                .corner_radius(egui::CornerRadius::same(3))
                                .inner_margin(Margin {
                                    left: 8,
                                    right: 8,
                                    top: 4,
                                    bottom: 4,
                                })
                                .show(ui, |ui| {
                                    ui.set_min_width(field_w - 24.0);
                                    ui.add(
                                        egui::Label::new(RichText::new(sug).size(12.0).color(tc))
                                            .truncate(),
                                    )
                                });
                            let rr = ui.interact(
                                row.response.rect,
                                egui::Id::new("exc_sug").with(i as u32),
                                egui::Sense::click(),
                            );
                            if rr.hovered() {
                                hov = Some(i);
                            }
                            if rr.clicked() {
                                exc_sel = Some(sug.clone());
                            }
                        }
                    });
                if let Some(i) = hov {
                    self.exc_suggest_idx = Some(i);
                }
                if let Some(v) = exc_sel.clone() {
                    picked_exc = Some(v);
                    egui::Popup::close_id(ui.ctx(), exc_popup_id);
                    self.exc_suggest_idx = None;
                }
            }
        }
        if let Some(ref er) = exc_resp_outer {
            if er.lost_focus() && exc_sel.is_none() {
                egui::Popup::close_id(ui.ctx(), exc_popup_id);
                self.exc_suggest_idx = None;
            }
        }

        if let Some(d) = picked_dir {
            self.params.directory = d;
        }
        if let Some(p) = picked_pattern {
            self.params.pattern = p;
            self.focus_pattern = true;
            self.pat_suppress_popup_open = true; // suppress reopen when focus returns
        }
        if let Some(v) = picked_inc {
            self.params.file_glob = v;
        }
        if let Some(v) = picked_exc {
            self.params.exclude_glob = v;
        }
    }

    fn recent_dirs(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut dirs = Vec::new();
        for entry in self.history.entries.iter().rev() {
            let dir = &entry.params.directory;
            if !dir.is_empty() && seen.insert(dir.clone()) {
                dirs.push(dir.clone());
                if dirs.len() >= 10 {
                    break;
                }
            }
        }
        dirs
    }

    fn recent_patterns(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut pats = Vec::new();
        for entry in self.history.entries.iter().rev() {
            let p = &entry.params.pattern;
            if !p.is_empty() && seen.insert(p.clone()) {
                pats.push(p.clone());
                if pats.len() >= 10 {
                    break;
                }
            }
        }
        pats
    }

    fn recent_includes(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut vals = Vec::new();
        for entry in self.history.entries.iter().rev() {
            let v = &entry.params.file_glob;
            if !v.is_empty() && seen.insert(v.clone()) {
                vals.push(v.clone());
                if vals.len() >= 10 {
                    break;
                }
            }
        }
        vals
    }

    fn recent_excludes(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut vals = Vec::new();
        for entry in self.history.entries.iter().rev() {
            let v = &entry.params.exclude_glob;
            if !v.is_empty() && seen.insert(v.clone()) {
                vals.push(v.clone());
                if vals.len() >= 10 {
                    break;
                }
            }
        }
        vals
    }

    /// Icon toggle button with tooltip (used for Settings / History / Replace).
    fn icon_toggle(&mut self, ui: &mut Ui, field: &str, icon: &str, tooltip: &str) {
        let pal = self.pal;
        let active = match field {
            "show_settings" => self.is_settings_active(),
            "show_history" => self.show_history,
            "show_replace" => self.show_replace,
            _ => false,
        };

        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(24.0), egui::Sense::click());
        let resp = resp
            .on_hover_text(tooltip)
            .on_hover_cursor(egui::CursorIcon::PointingHand);

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            if active || resp.hovered() {
                let bg = if active {
                    pal.bg_surface0
                } else {
                    pal.bg_surface0.gamma_multiply(0.6)
                };
                painter.rect_filled(rect.shrink(2.0), egui::CornerRadius::same(4), bg);
            }
            let color = if active {
                pal.accent
            } else if resp.hovered() {
                pal.text
            } else {
                pal.subtext
            };
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                icon,
                egui::FontId::new(16.0, egui::FontFamily::Name("Icons".into())),
                color,
            );
        }

        if resp.clicked() {
            match field {
                "show_settings" => self.open_settings_tab(),
                "show_history" => self.show_history = !self.show_history,
                "show_replace" => self.show_replace = !self.show_replace,
                _ => {}
            }
        }
    }

    fn show_replace_bar(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);

            // "Find" side – show current search pattern (read-only badge)
            ui.label(RichText::new("Find:").color(pal.subtext).size(12.0));
            let pat_display = if self.params.pattern.is_empty() {
                RichText::new("(no pattern)")
                    .color(pal.muted)
                    .monospace()
                    .size(12.0)
            } else {
                RichText::new(&self.params.pattern)
                    .color(pal.accent)
                    .monospace()
                    .size(12.0)
            };
            egui::Frame::NONE
                .fill(pal.bg_surface0)
                .corner_radius(egui::CornerRadius::same(3))
                .inner_margin(egui::Margin {
                    left: 6,
                    right: 6,
                    top: 2,
                    bottom: 2,
                })
                .show(ui, |ui| {
                    ui.label(pat_display);
                });

            // Arrow separator
            ui.label(RichText::new("→").color(pal.muted).size(14.0));

            // "Replace" side – editable input
            ui.label(RichText::new("Replace:").color(pal.subtext).size(12.0));
            ui.add(
                egui::TextEdit::singleline(&mut self.params.replace_text)
                    .hint_text(
                        RichText::new("replacement text")
                            .color(pal.placeholder)
                            .italics(),
                    )
                    .desired_width(180.0)
                    .font(egui::TextStyle::Monospace),
            );

            ui.separator();

            // Scope selection
            ui.label(RichText::new("Scope:").color(pal.subtext).size(12.0));
            ui.radio_value(
                &mut self.params.replace_scope,
                crate::models::ReplaceScope::Selected,
                RichText::new("Selected").size(12.0),
            )
            .on_hover_text("Apply replacement only to the selected files");
            ui.radio_value(
                &mut self.params.replace_scope,
                crate::models::ReplaceScope::All,
                RichText::new("All").size(12.0),
            )
            .on_hover_text("Apply replacement to ALL matched files");

            ui.separator();

            let has_result = self.current_result.is_some();
            let is_selected_scope =
                self.params.replace_scope == crate::models::ReplaceScope::Selected;
            let preview_has_targets = if is_selected_scope {
                !self.selected_files.is_empty()
            } else {
                self.current_result
                    .as_ref()
                    .is_some_and(|r| !r.files.is_empty())
            };
            let preview_ready =
                preview_has_targets && has_result && !self.params.pattern.is_empty();
            let preview_tip = if is_selected_scope {
                "Preview replacement for selected files"
            } else {
                "Preview replacement for all matched files (up to 20)"
            };

            if ui
                .add_enabled(
                    preview_ready,
                    egui::Button::new(RichText::new("Preview").color(pal.text).size(12.0)),
                )
                .on_hover_text(preview_tip)
                .clicked()
            {
                self.do_replace_preview();
            }

            let files_to_replace = self.get_files_to_replace();
            let replace_ready =
                has_result && !self.params.pattern.is_empty() && !files_to_replace.is_empty();

            if ui
                .add_enabled(
                    replace_ready,
                    egui::Button::new(RichText::new("Replace").color(pal.bg_mantle).size(12.0))
                        .fill(pal.red),
                )
                .on_hover_text("Apply replacement to the selected scope")
                .clicked()
            {
                self.do_replace_all();
            }
        });
    }
}

// ── File panel ────────────────────────────────────────────────────────────────
impl GrepApp {
    fn show_file_panel(&mut self, ui: &mut Ui) {
        let pal = self.pal;

        // Extract lightweight (path, match_count) pairs — avoids cloning LineMatch content.
        let base = self
            .current_result
            .as_ref()
            .map(|r| PathBuf::from(&r.params.directory))
            .unwrap_or_default();
        let file_entries: Vec<(PathBuf, usize)> = self
            .current_result
            .as_ref()
            .map(|r| {
                r.files
                    .iter()
                    .map(|f| {
                        (
                            f.path.clone(),
                            f.matches.iter().filter(|m| m.is_match).count(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        let filter_lower = self.file_filter.to_lowercase();

        // Filtered indices into file_entries (no data copying).
        let filtered_indices: Vec<usize> = if filter_lower.is_empty() {
            (0..file_entries.len()).collect()
        } else {
            file_entries
                .iter()
                .enumerate()
                .filter(|(_, (path, _))| {
                    let rel = path.strip_prefix(&base).unwrap_or(path);
                    rel.to_string_lossy().to_lowercase().contains(&filter_lower)
                })
                .map(|(i, _)| i)
                .collect()
        };

        let all_file_count = file_entries.len();
        let filtered_count = filtered_indices.len();

        // ── Header row ─────────────────────────────────────────────────
        egui::Frame::NONE
            .fill(pal.bg_base)
            .inner_margin(Margin {
                left: 10,
                right: 8,
                top: 8,
                bottom: 4,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let count_text = if filter_lower.is_empty() {
                        format!("Files  {}", all_file_count)
                    } else {
                        format!("Files  {}/{}", filtered_count, all_file_count)
                    };
                    ui.label(RichText::new(count_text).color(pal.subtext).size(12.0));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                        if !file_entries.is_empty() {
                            if ui
                                .small_button(RichText::new("None").color(pal.muted))
                                .on_hover_text("Deselect all")
                                .clicked()
                            {
                                self.selected_files.clear();
                            }
                            if ui
                                .small_button(RichText::new("All").color(pal.accent))
                                .on_hover_text("Select all files")
                                .clicked()
                            {
                                self.selected_files =
                                    file_entries.iter().map(|(p, _)| p.clone()).collect();
                            }
                            ui.separator();
                        }
                        let flat_active = matches!(self.view_mode, ViewMode::Flat);
                        let flat_color = if flat_active { pal.accent } else { pal.subtext };
                        if ui
                            .add_sized(
                                [24.0, 24.0],
                                egui::Button::selectable(
                                    flat_active,
                                    icon_rt(icons::LIST_FLAT, 15.0, flat_color),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Flat file list")
                            .clicked()
                        {
                            self.view_mode = ViewMode::Flat;
                        }
                        let tree_active = matches!(self.view_mode, ViewMode::Tree);
                        let tree_color = if tree_active { pal.accent } else { pal.subtext };
                        if ui
                            .add_sized(
                                [24.0, 24.0],
                                egui::Button::selectable(
                                    tree_active,
                                    icon_rt(icons::LIST_TREE, 15.0, tree_color),
                                )
                                .frame(false),
                            )
                            .on_hover_text("Tree view")
                            .clicked()
                        {
                            self.view_mode = ViewMode::Tree;
                        }
                    });
                });
            });

        // ── Root dir indicator (single or multi-root) ─────────────────
        if let Some(result) = &self.current_result {
            let all_roots: Vec<&str> = std::iter::once(result.params.directory.as_str())
                .chain(result.params.roots.iter().map(|s| s.as_str()))
                .filter(|s| !s.is_empty())
                .collect();
            if !all_roots.is_empty() {
                let (display, tooltip) = if all_roots.len() == 1 {
                    (
                        format!("in  {}", short_dir(all_roots[0])),
                        all_roots[0].to_string(),
                    )
                } else {
                    (
                        format!("in  {} dirs", all_roots.len()),
                        all_roots.join("\n"),
                    )
                };
                egui::Frame::NONE
                    .fill(pal.bg_base)
                    .inner_margin(Margin {
                        left: 10,
                        right: 8,
                        top: 0,
                        bottom: 3,
                    })
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(display)
                                .color(pal.muted)
                                .size(11.0)
                                .monospace(),
                        )
                        .on_hover_text(tooltip);
                    });
            }
        }

        // ── Filter row (separate from header frame to avoid height overflow) ──
        egui::Frame::NONE
            .fill(pal.bg_base)
            .inner_margin(Margin {
                left: 8,
                right: 8,
                top: 0,
                bottom: 6,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Always reserve a fixed btn_w so the layout width never changes
                    // as the user types (prevents panel-expansion feedback loop).
                    let btn_w = 22.0;
                    let interact_h = ui.spacing().interact_size.y;
                    let text_w =
                        (ui.available_width() - btn_w - ui.spacing().item_spacing.x).max(40.0);
                    let filter_resp = ui.add_sized(
                        [text_w, interact_h],
                        egui::TextEdit::singleline(&mut self.file_filter).hint_text(
                            RichText::new("filter files...")
                                .color(pal.placeholder)
                                .italics(),
                        ),
                    );
                    // Consume Tab/Shift+Tab to keep filter in its own group and
                    // prevent egui's default ordering from jumping to Inc.
                    if filter_resp.has_focus() {
                        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
                            ui.ctx()
                                .memory_mut(|m| m.request_focus(egui::Id::new("dir_input")));
                        } else if ui
                            .input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab))
                        {
                        }
                    }
                    // Clear button: always occupies btn_w; invisible when filter is empty.
                    let clear_color = if self.file_filter.is_empty() {
                        Color32::TRANSPARENT
                    } else {
                        pal.muted
                    };
                    let cleared = ui
                        .add_sized(
                            [btn_w, interact_h],
                            egui::Button::new(RichText::new("×").color(clear_color)).frame(false),
                        )
                        .clicked();
                    if !self.file_filter.is_empty() && cleared {
                        self.file_filter.clear();
                    }
                });
            });

        // Separator line
        ui.painter().hline(
            ui.available_rect_before_wrap().x_range(),
            ui.cursor().top(),
            Stroke::new(1.0, pal.bg_surface0),
        );

        if all_file_count == 0 {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No results yet").color(pal.muted).size(13.0));
            });
            return;
        }

        if filtered_count == 0 {
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("No files match filter")
                        .color(pal.muted)
                        .size(12.0),
                );
            });
            return;
        }

        // ── File list ──────────────────────────────────────────────────
        let editor_cmd = self.config.editor_command.clone();

        match self.view_mode {
            ViewMode::Flat => {
                let mut open_req: Option<PathBuf> = None;
                let row_height = 22.0;
                ScrollArea::vertical().auto_shrink([false; 2]).show_rows(
                    ui,
                    row_height,
                    filtered_count,
                    |ui, row_range| {
                        ui.add_space(4.0);
                        egui::Frame::NONE
                            .inner_margin(Margin {
                                left: 6,
                                right: 0,
                                top: 0,
                                bottom: 0,
                            })
                            .show(ui, |ui| {
                                for row_idx in row_range {
                                    let entry_idx = filtered_indices[row_idx];
                                    let (path, match_count) = &file_entries[entry_idx];
                                    let selected = self.selected_files.contains(path);
                                    let rel = path
                                        .strip_prefix(&base)
                                        .unwrap_or(path)
                                        .to_string_lossy()
                                        .to_string();
                                    let row =
                                        file_row(ui, pal, &rel, selected, *match_count, Some(&rel));
                                    if row.clicked {
                                        toggle_selection(&mut self.selected_files, path);
                                    }
                                    if row.double_clicked {
                                        open_req = Some(path.clone());
                                    }
                                }
                            });
                        ui.add_space(8.0);
                    },
                );
                if let Some(p) = open_req {
                    open_in_editor(&p, None, &editor_cmd);
                }
            }
            ViewMode::Tree => {
                // Build a filtered (path, match_count) slice for tree rendering.
                // When there is no filter, move file_entries directly (zero extra copy).
                // When filtered, clone only the matching subset — paths only, no match content.
                let tree_entries: Vec<(PathBuf, usize)> = if filter_lower.is_empty() {
                    file_entries
                } else {
                    filtered_indices
                        .iter()
                        .map(|&i| (file_entries[i].0.clone(), file_entries[i].1))
                        .collect()
                };
                let ctx = ui.ctx().clone();
                let flat_items = build_flat_tree(&tree_entries, &base, &ctx);
                let mut open_req: Option<PathBuf> = None;
                const TREE_ROW_H: f32 = 22.0;
                ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show_rows(ui, TREE_ROW_H, flat_items.len(), |ui, row_range| {
                        ui.add_space(4.0);
                        for idx in row_range {
                            match &flat_items[idx] {
                                FlatTreeItem::Dir {
                                    name,
                                    path,
                                    rel_path,
                                    indent,
                                    id,
                                    is_open,
                                } => {
                                    let (id, is_open) = (*id, *is_open);
                                    let indent_px = *indent as f32 * 14.0 + 6.0;
                                    let icon = if is_open { "▾" } else { "▸" };
                                    ui.horizontal(|ui| {
                                        ui.add_space(indent_px);
                                        let resp = ui
                                            .add(
                                                egui::Label::new(
                                                    RichText::new(format!("{icon}  {name}"))
                                                        .color(pal.subtext)
                                                        .size(12.5),
                                                )
                                                .sense(egui::Sense::click())
                                                .truncate(),
                                            )
                                            .on_hover_text(rel_path.as_str());
                                        if resp.clicked() {
                                            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                                                &ctx, id, true,
                                            );
                                            state.set_open(!is_open);
                                            state.store(&ctx);
                                        }
                                        if resp.double_clicked() && !editor_cmd.is_empty() {
                                            open_req = Some(path.clone());
                                        }
                                    });
                                }
                                FlatTreeItem::File {
                                    name,
                                    path,
                                    rel_path,
                                    match_count,
                                    indent,
                                } => {
                                    let indent_px = *indent as f32 * 14.0 + 6.0;
                                    ui.horizontal(|ui| {
                                        ui.add_space(indent_px);
                                        let sel =
                                            self.selected_files.contains(path.as_path());
                                        let row = file_row(
                                            ui,
                                            pal,
                                            name,
                                            sel,
                                            *match_count,
                                            Some(rel_path.as_str()),
                                        );
                                        if row.clicked {
                                            toggle_selection(
                                                &mut self.selected_files,
                                                path,
                                            );
                                        }
                                        if row.double_clicked
                                            && !editor_cmd.is_empty()
                                        {
                                            open_req = Some(path.clone());
                                        }
                                    });
                                }
                            }
                        }
                        ui.add_space(8.0);
                    });
                if let Some(p) = open_req {
                    open_in_editor(&p, None, &editor_cmd);
                }
            }
        }
    }
}

// ── Content panel ─────────────────────────────────────────────────────────────
impl GrepApp {
    fn show_content_panel(&mut self, ui: &mut Ui) {
        let pal = self.pal;

        if let Some(err) = &self.last_search_error {
            ui.centered_and_justified(|ui| {
                egui::Frame::NONE
                    .fill(pal.bg_surface0)
                    .stroke(egui::Stroke::new(1.0, pal.red))
                    .inner_margin(Margin::same(16))
                    .corner_radius(egui::CornerRadius::same(6))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new(icons::WARNING).color(pal.red).size(24.0));
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("Search Error")
                                    .color(pal.text)
                                    .strong()
                                    .size(16.0),
                            );
                            ui.add_space(8.0);
                            ui.label(RichText::new(err).color(pal.subtext).monospace().size(13.0));
                        });
                    });
            });
            return;
        }

        if self.current_result.is_none() {
            // No search has been run yet — show a hero / onboarding card.
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(24.0);
                    ui.label(
                        RichText::new("aero-grep")
                            .color(pal.accent)
                            .size(28.0)
                            .strong(),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Fast full-text search across your codebase")
                            .color(pal.subtext)
                            .size(13.0),
                    );
                    ui.add_space(28.0);

                    let shortcut_color = pal.muted;
                    let key_color = pal.accent;
                    let desc_color = pal.text;

                    let shortcuts: &[(&str, &str)] = &[
                        ("Enter", "Run search"),
                        ("⌘K / Ctrl+K", "Command palette"),
                        ("F3 / Ctrl+G", "Next match"),
                        ("⌘F / Ctrl+F", "Focus pattern"),
                    ];
                    egui::Frame::NONE
                        .fill(pal.bg_surface0)
                        .inner_margin(Margin::symmetric(20, 14))
                        .corner_radius(egui::CornerRadius::same(8))
                        .show(ui, |ui| {
                            ui.set_max_width(280.0);
                            for (key, desc) in shortcuts {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(*key).color(key_color).monospace().size(12.0),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                RichText::new(*desc).color(desc_color).size(12.0),
                                            );
                                        },
                                    );
                                });
                                ui.add_space(4.0);
                            }
                            ui.add_space(2.0);
                            ui.label(
                                RichText::new("⚙  settings · history top-right")
                                    .color(shortcut_color)
                                    .size(11.0),
                            );
                        });
                });
            });
            return;
        }

        if self.selected_files.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Select files from the list to view matches")
                        .color(pal.muted)
                        .size(14.0),
                );
            });
            return;
        }

        let Some(result) = &self.current_result else {
            return;
        };

        let mut copy_all_req = false;

        // ── Search params header ──────────────────────────────────────────────
        egui::Frame::NONE
            .fill(pal.bg_base)
            .inner_margin(Margin {
                left: 12,
                right: 12,
                top: 6,
                bottom: 6,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);

                    // Pattern
                    let pat = &result.params.pattern;
                    let display_pat = if pat.is_empty() {
                        "(empty)".to_string()
                    } else if pat.chars().count() > 30 {
                        format!("\"{}...\"", pat.chars().take(30).collect::<String>())
                    } else {
                        format!("\"{}\"", pat)
                    };
                    ui.label(
                        RichText::new(display_pat)
                            .color(pal.text)
                            .strong()
                            .monospace()
                            .size(12.0),
                    )
                    .on_hover_text(pat);

                    ui.label(RichText::new("in").color(pal.muted).size(11.0));

                    // Dir
                    let dir_path = &result.params.directory;
                    let display_dir = short_dir(dir_path);
                    ui.label(
                        RichText::new(display_dir)
                            .color(pal.text)
                            .monospace()
                            .size(11.0),
                    )
                    .on_hover_text(dir_path);

                    // Flags
                    if result.params.is_regex {
                        ui.label(RichText::new("[Regex]").color(pal.accent).size(10.0))
                            .on_hover_text("Regular expression matching");
                    }
                    if result.params.case_sensitive {
                        ui.label(RichText::new("[Case]").color(pal.accent).size(10.0))
                            .on_hover_text("Case sensitive matching");
                    }
                    if result.params.word_boundary {
                        ui.label(RichText::new("[Word]").color(pal.accent).size(10.0))
                            .on_hover_text("Whole word matching");
                    }
                    if result.params.context_lines > 0 {
                        ui.label(
                            RichText::new(format!("[Ctx:{}]", result.params.context_lines))
                                .color(pal.accent)
                                .size(10.0),
                        )
                        .on_hover_text(format!(
                            "{} context lines shown",
                            result.params.context_lines
                        ));
                    }
                    if !result.params.file_glob.is_empty() {
                        let glob = &result.params.file_glob;
                        let display_glob = if glob.chars().count() > 15 {
                            format!("{}...", glob.chars().take(15).collect::<String>())
                        } else {
                            glob.clone()
                        };
                        ui.label(
                            RichText::new(format!("[Inc:{}]", display_glob))
                                .color(pal.accent)
                                .size(10.0),
                        )
                        .on_hover_text(format!("Included glob filter: {}", glob));
                    }
                    if !result.params.exclude_glob.is_empty() {
                        let excl = &result.params.exclude_glob;
                        let display_excl = if excl.chars().count() > 15 {
                            format!("{}...", excl.chars().take(15).collect::<String>())
                        } else {
                            excl.clone()
                        };
                        ui.label(
                            RichText::new(format!("[Exc:{}]", display_excl))
                                .color(pal.accent)
                                .size(10.0),
                        )
                        .on_hover_text(format!("Excluded glob filter: {}", excl));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                        {
                            let (wrap_rect, wrap_resp) =
                                ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::click());
                            let wrap_resp = wrap_resp
                                .on_hover_text(if self.config.wrap_lines {
                                    "Word wrap on (click to disable)"
                                } else {
                                    "Word wrap off (click to enable)"
                                })
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if ui.is_rect_visible(wrap_rect) {
                                let painter = ui.painter();
                                if self.config.wrap_lines || wrap_resp.hovered() {
                                    let bg = if self.config.wrap_lines {
                                        pal.bg_surface0
                                    } else {
                                        pal.bg_surface0.gamma_multiply(0.6)
                                    };
                                    painter.rect_filled(
                                        wrap_rect.shrink(2.0),
                                        egui::CornerRadius::same(4),
                                        bg,
                                    );
                                }
                                let wrap_color = if self.config.wrap_lines {
                                    pal.accent
                                } else if wrap_resp.hovered() {
                                    pal.text
                                } else {
                                    pal.subtext
                                };
                                let wrap_icon = if self.config.wrap_lines {
                                    icons::WRAP
                                } else {
                                    icons::WRAP_OFF
                                };
                                painter.text(
                                    wrap_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    wrap_icon,
                                    egui::FontId::new(15.0, egui::FontFamily::Name("Icons".into())),
                                    wrap_color,
                                );
                            }
                            if wrap_resp.clicked() {
                                self.config.wrap_lines = !self.config.wrap_lines;
                                let _ = self.config.save();
                            }
                        }

                        let is_active = self.copied_flash.is_some();
                        if ghost_icon_button(ui, pal, icons::COPY, is_active)
                            .on_hover_text("Copy all selected match results to clipboard")
                            .clicked()
                        {
                            copy_all_req = true;
                        }
                    });
                });
            });

        if copy_all_req {
            let selected_fm: Vec<&FileMatch> = self
                .selected_files
                .iter()
                .filter_map(|p| result.files.iter().find(|f| &f.path == p))
                .collect();
            let text = self.format_matches_to_string(&selected_fm, &result.params);
            if !text.is_empty() {
                self.copy_text(ui.ctx(), text);
            }
        }

        let Some(result) = &self.current_result else {
            return;
        };

        if result.truncated {
            egui::Frame::NONE
                .fill(pal.bg_surface0)
                .inner_margin(Margin::symmetric(12, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icons::WARNING).color(pal.yellow));
                        ui.label(
                            RichText::new("Search results truncated: maximum file limit reached. Adjust limit in Settings.")
                                .color(pal.yellow)
                                .size(11.0),
                        );
                    });
                });
        }

        // Thin divider between search header and matches
        ui.painter().hline(
            ui.available_rect_before_wrap().x_range(),
            ui.cursor().top(),
            Stroke::new(1.0, pal.bg_surface0),
        );
        ui.add_space(1.0);

        let selected: Vec<PathBuf> = self.selected_files.iter().cloned().collect();
        let files: Vec<FileMatch> = selected
            .iter()
            .filter_map(|p| result.files.iter().find(|f| &f.path == p).cloned())
            .collect();

        let dir = result.params.directory.clone();
        let editor_cmd = self.config.editor_command.clone();

        // horizontalの無限幅判定を回避するため、ScrollAreaの外で可視幅を取得
        let available_w = ui.available_width();
        let wrap_width = if self.config.wrap_lines {
            // Subtract gutter (~53px) + spacing (12px) + scrollbar (~18px) + buffer (12px)
            Some((available_w - 95.0).max(100.0))
        } else {
            None
        };

        // ── Flatten rendering items for virtual scrolling ──────────────────
        #[derive(Clone)]
        enum RenderItem<'a> {
            FileHeader {
                fm: &'a FileMatch,
                rel: String,
                is_collapsed: bool,
                match_count: usize,
            },
            GapSeparator,
            MatchLine {
                fm: &'a FileMatch,
                lm: &'a LineMatch,
                is_current: bool,
            },
        }

        let current_active_line = self.current_match.and_then(|(f_idx, m_idx)| {
            let res = self.current_result.as_ref()?;
            let fm = res.files.get(f_idx)?;
            let lm = fm.matches.iter().filter(|m| m.is_match).nth(m_idx)?;
            Some((fm.path.clone(), lm.line_number))
        });

        let mut items = Vec::new();
        for fm in &files {
            let rel = fm
                .path
                .strip_prefix(&dir)
                .unwrap_or(&fm.path)
                .to_string_lossy()
                .to_string();
            let is_collapsed = self.collapsed_files.contains(&fm.path);
            let match_count = fm.matches.iter().filter(|m| m.is_match).count();

            items.push(RenderItem::FileHeader {
                fm,
                rel,
                is_collapsed,
                match_count,
            });

            if !is_collapsed {
                let mut prev_line_number: Option<usize> = None;
                for lm in &fm.matches {
                    if let Some(prev) = prev_line_number {
                        if lm.line_number > prev + 1 {
                            items.push(RenderItem::GapSeparator);
                        }
                    }
                    prev_line_number = Some(lm.line_number);

                    let is_current = current_active_line
                        .as_ref()
                        .map(|(path, line_num)| {
                            path == &fm.path && lm.is_match && lm.line_number == *line_num
                        })
                        .unwrap_or(false);

                    items.push(RenderItem::MatchLine { fm, lm, is_current });
                }
            }
        }

        let mut copy_text: Option<String> = None;
        let mut toggle_collapse: Option<PathBuf> = None;
        let mut set_current_match: Option<(usize, usize)> = None;
        let mut clear_scroll_to_current = false;
        let mut set_copied_file: Option<PathBuf> = None;

        let scroll_target_idx = if self.scroll_to_current {
            current_active_line
                .as_ref()
                .and_then(|(current_path, current_line_num)| {
                    items.iter().position(|item| {
                        if let RenderItem::MatchLine { fm, lm, .. } = item {
                            &fm.path == current_path && lm.line_number == *current_line_num
                        } else {
                            false
                        }
                    })
                })
        } else {
            None
        };

        let scroll_area = if self.config.wrap_lines {
            ScrollArea::vertical()
        } else {
            ScrollArea::both()
        };
        scroll_area.auto_shrink([false; 2]).show(ui, |ui| {
            ui.add_space(4.0);
            for (idx, item) in items.iter().enumerate() {
                if Some(idx) == scroll_target_idx {
                    ui.scroll_to_cursor(Some(egui::Align::Center));
                    clear_scroll_to_current = true;
                }
                match item {
                    RenderItem::FileHeader {
                        fm,
                        rel,
                        is_collapsed,
                        match_count,
                    } => {
                        let chevron = if *is_collapsed { "▶" } else { "▼" };

                        // Reserve background paint slot; fill after content height is known
                        let bg_painter = ui.painter().clone();
                        let bg_shape_idx = bg_painter.add(egui::Shape::Noop);
                        let row_top = ui.cursor().top();

                        let header_resp = ui.horizontal(|ui| {
                            // Cap layout width to the visible area so right_to_left
                            // sub-layout works correctly inside ScrollArea::both().
                            ui.set_max_width(ui.clip_rect().width());
                            ui.spacing_mut().item_spacing = egui::Vec2::new(8.0, 0.0);
                            ui.add_space(12.0); // Left margin

                            let chevron_resp = ui.add(
                                egui::Label::new(
                                    RichText::new(chevron).color(pal.muted).size(11.0),
                                )
                                .sense(egui::Sense::click()),
                            );
                            if chevron_resp.clicked() {
                                toggle_collapse = Some(fm.path.clone());
                            }
                            chevron_resp.on_hover_text("Click to collapse/expand");

                            ui.label(RichText::new("  ").size(13.0).background_color(pal.accent));
                            ui.add_space(4.0);

                            let right_reserve = 60.0_f32; // Reserve space for sticky copy button on the right
                            let left_used = 12.0 + 16.0 + 4.0 + 8.0;
                            let path_max_w =
                                (ui.available_width() - right_reserve - left_used).max(60.0);

                            let display_rel = truncate_path(rel, path_max_w, 7.5);

                            let path_resp = ghost_button(
                                ui,
                                pal,
                                RichText::new(&display_rel)
                                    .color(pal.text)
                                    .monospace()
                                    .size(13.0),
                            );
                            if path_resp.double_clicked() && !editor_cmd.is_empty() {
                                open_in_editor(&fm.path, None, &editor_cmd);
                            } else if path_resp.clicked() {
                                toggle_collapse = Some(fm.path.clone());
                            }
                            if display_rel != *rel {
                                path_resp.on_hover_text(rel);
                            }

                            ui.label(
                                RichText::new(format!("  {} matches", match_count))
                                    .color(pal.muted)
                                    .size(11.0),
                            );

                            // Always display clipboard button sticky to the right edge of visible row
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add_space(8.0); // Right margin
                                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                                    let is_active = self
                                        .copied_file_flash
                                        .as_ref()
                                        .map(|(p, _)| p == &fm.path)
                                        .unwrap_or(false);
                                    let copy_btn =
                                        ghost_icon_button(ui, pal, icons::COPY, is_active)
                                            .on_hover_text("Copy all matches for this file");
                                    if copy_btn.clicked() {
                                        copy_text = Some(
                                            self.format_matches_to_string(&[fm], &result.params),
                                        );
                                        set_copied_file = Some(fm.path.clone());
                                    }
                                },
                            );
                        });

                        // Paint background spanning the full visible width at actual height
                        bg_painter.set(
                            bg_shape_idx,
                            egui::Shape::rect_filled(
                                egui::Rect::from_min_max(
                                    egui::pos2(ui.clip_rect().min.x, row_top),
                                    egui::pos2(
                                        ui.clip_rect().max.x,
                                        header_resp.response.rect.max.y,
                                    ),
                                ),
                                0.0,
                                pal.bg_mantle,
                            ),
                        );
                    }
                    RenderItem::GapSeparator => {
                        let top = ui.cursor().top();
                        let rect = ui.available_rect_before_wrap();
                        // Subtle 1px line aligned with the content column (after gutter)
                        ui.painter().hline(
                            egui::Rangef::new(rect.left() + 53.0, rect.right()),
                            top + 4.0,
                            Stroke::new(1.0, pal.bg_surface0),
                        );
                        ui.add_space(8.0);
                    }
                    RenderItem::MatchLine { fm, lm, is_current } => {
                        let frame = if *is_current {
                            egui::Frame::NONE
                                .fill(pal.bg_surface1)
                                .corner_radius(CornerRadius::same(3))
                                .inner_margin(Margin {
                                    left: 4,
                                    right: 4,
                                    top: 1,
                                    bottom: 1,
                                })
                        } else {
                            egui::Frame::NONE
                        };

                        frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::ZERO;

                                let ln_color = if lm.is_match {
                                    let a = pal.subtext;
                                    let b = pal.accent;
                                    Color32::from_rgb(
                                        ((a.r() as u16 + b.r() as u16) / 2) as u8,
                                        ((a.g() as u16 + b.g() as u16) / 2) as u8,
                                        ((a.b() as u16 + b.b() as u16) / 2) as u8,
                                    )
                                } else {
                                    pal.muted
                                };
                                let rt = RichText::new(format!(" {:>4} ", lm.line_number))
                                    .monospace()
                                    .size(12.5)
                                    .color(ln_color);
                                let gutter =
                                    ui.add(egui::Label::new(rt).sense(egui::Sense::click()));
                                if gutter.clicked() {
                                    if lm.is_match {
                                        set_current_match = Some((
                                            result
                                                .files
                                                .iter()
                                                .position(|f| f.path == fm.path)
                                                .unwrap_or(0),
                                            fm.matches
                                                .iter()
                                                .filter(|m| m.is_match)
                                                .position(|m| m.line_number == lm.line_number)
                                                .unwrap_or(0),
                                        ));
                                    }
                                    if !editor_cmd.is_empty() {
                                        open_in_editor(&fm.path, Some(lm.line_number), &editor_cmd);
                                    }
                                }
                                if gutter.hovered() && lm.is_match {
                                    let r = gutter.rect;
                                    ui.painter().hline(
                                        egui::Rangef::new(r.left(), r.right()),
                                        r.bottom() - 1.0,
                                        Stroke::new(1.0, ln_color),
                                    );
                                }
                                let gutter = gutter.on_hover_cursor(egui::CursorIcon::PointingHand);
                                gutter.on_hover_text("Click to open at this line");

                                ui.add_space(4.0);
                                let r = ui.available_rect_before_wrap();
                                ui.painter().vline(
                                    r.left(),
                                    egui::Rangef::new(r.top(), r.bottom()),
                                    Stroke::new(1.0, pal.bg_surface0),
                                );
                                ui.add_space(8.0);

                                let wrap_mode = if wrap_width.is_some() {
                                    egui::TextWrapMode::Wrap
                                } else {
                                    egui::TextWrapMode::Extend
                                };
                                if lm.is_match {
                                    let job = build_highlighted_line(
                                        &lm.content,
                                        &lm.ranges,
                                        pal,
                                        wrap_width,
                                    );
                                    ui.add(
                                        egui::Label::new(job).selectable(true).wrap_mode(wrap_mode),
                                    );
                                } else {
                                    let job = build_context_line(&lm.content, pal, wrap_width);
                                    ui.add(
                                        egui::Label::new(job).selectable(true).wrap_mode(wrap_mode),
                                    );
                                }
                            });
                        });
                    }
                }
            }
        });

        if let Some(path) = toggle_collapse {
            if self.collapsed_files.contains(&path) {
                self.collapsed_files.remove(&path);
            } else {
                self.collapsed_files.insert(path);
            }
        }
        if let Some(text) = copy_text {
            self.copy_text(ui.ctx(), text);
        }
        if let Some(path) = set_copied_file {
            self.copied_file_flash = Some((path, std::time::Instant::now()));
        }
        if let Some(target) = set_current_match {
            self.current_match = Some(target);
            self.scroll_to_current = true;
        }
        if clear_scroll_to_current {
            self.scroll_to_current = false;
        }
    }
}

// ── History panel ─────────────────────────────────────────────────────────────
impl GrepApp {
    fn show_history_panel(&mut self, ui: &mut Ui) {
        let pal = self.pal;

        egui::Frame::NONE
            .fill(pal.bg_surface0)
            .inner_margin(Margin {
                left: 12,
                right: 12,
                top: 6,
                bottom: 6,
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("History").color(pal.accent).size(13.0));
                    ui.label(
                        RichText::new(format!("{} entries", self.history.entries.len()))
                            .color(pal.muted)
                            .size(11.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button(RichText::new("Clear All").color(pal.red))
                            .clicked()
                        {
                            self.history.clear();
                        }
                    });
                });
                ui.add_space(4.0);
                let filter_w = (ui.available_width()).max(60.0);
                ui.add_sized(
                    [filter_w, 20.0],
                    egui::TextEdit::singleline(&mut self.history_filter)
                        .hint_text("Filter by pattern or directory...")
                        .font(egui::TextStyle::Small),
                );
            });

        if self.history.entries.is_empty() {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No history yet").color(pal.muted).size(12.0));
            });
            return;
        }

        let filter_lower = self.history_filter.to_lowercase();
        let entries: Vec<_> = self
            .history
            .entries
            .iter()
            .filter(|e| {
                if filter_lower.is_empty() {
                    return true;
                }
                e.params.pattern.to_lowercase().contains(&filter_lower)
                    || e.params.directory.to_lowercase().contains(&filter_lower)
            })
            .cloned()
            .collect();
        let mut to_load: Option<usize> = None;
        let mut to_rerun: Option<usize> = None;
        let mut to_remove: Option<u64> = None;

        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
                for (i, entry) in entries.iter().enumerate() {
                    // Divider between entries (skip before first)
                    if i > 0 {
                        ui.painter().hline(
                            ui.available_rect_before_wrap().x_range(),
                            ui.cursor().top(),
                            Stroke::new(1.0, pal.bg_surface0),
                        );
                    }

                    egui::Frame::NONE
                        .inner_margin(Margin {
                            left: 12,
                            right: 8,
                            top: 5,
                            bottom: 5,
                        })
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            // Line 1: pattern (quoted)
                            ui.label(
                                RichText::new(format!("\"{}\"", entry.params.pattern))
                                    .color(pal.accent)
                                    .monospace()
                                    .size(12.5),
                            );
                            // Line 2: directory path alone (full width, truncated)
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&entry.params.directory)
                                        .color(pal.text)
                                        .size(12.0),
                                )
                                .truncate(),
                            );
                            // Line 3: stats (left) + action buttons (right)
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} files · {} matches · {}",
                                        entry.file_count(),
                                        entry.total_matches,
                                        format_ts(&entry.timestamp)
                                    ))
                                    .color(pal.muted)
                                    .size(10.5),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                                        if ui
                                            .add(
                                                egui::Button::new(icon_rt(
                                                    icons::TRASH,
                                                    14.0,
                                                    pal.red,
                                                ))
                                                .frame(false),
                                            )
                                            .on_hover_text("Delete entry")
                                            .clicked()
                                        {
                                            to_remove = Some(entry.id);
                                        }
                                        if ui
                                            .add(
                                                egui::Button::new(icon_rt(
                                                    icons::PLAY,
                                                    14.0,
                                                    pal.green,
                                                ))
                                                .frame(false),
                                            )
                                            .on_hover_text("Re-run search")
                                            .clicked()
                                        {
                                            to_rerun = Some(i);
                                        }
                                        if ui
                                            .add(
                                                egui::Button::new(icon_rt(
                                                    icons::HISTORY,
                                                    14.0,
                                                    pal.accent,
                                                ))
                                                .frame(false),
                                            )
                                            .on_hover_text("Load result")
                                            .clicked()
                                        {
                                            to_load = Some(i);
                                        }
                                    },
                                );
                            });
                        });
                }
                ui.add_space(8.0);
            });

        if let Some(i) = to_load {
            let e = entries[i].clone();
            self.load_history_entry(e);
        }
        if let Some(i) = to_rerun {
            self.rerun_history_entry(&entries[i].clone());
        }
        if let Some(id) = to_remove {
            self.history.remove(id);
        }
    }
}

// ── Settings window ───────────────────────────────────────────────────────────
impl GrepApp {
    fn show_settings_panel(&mut self, ui: &mut Ui) {
        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                egui::Frame::NONE
                    .inner_margin(Margin {
                        left: 32,
                        right: 32,
                        top: 24,
                        bottom: 32,
                    })
                    .show(ui, |ui| {
                        self.show_settings_window(ui);
                    });
            });
    }

    fn show_settings_window(&mut self, ui: &mut Ui) {
        let old_config = self.config.clone();
        let pal = self.pal;
        ui.spacing_mut().slider_width = 240.0;

        // ── Tab bar ───────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
            for (idx, label) in ["Appearance", "Search", "Editor", "Presets", "Export"]
                .iter()
                .enumerate()
            {
                let idx = idx as u8;
                let active = self.settings_tab == idx;
                let color = if active { pal.accent } else { pal.subtext };
                if ui
                    .add(egui::Button::selectable(
                        active,
                        RichText::new(*label).color(color).size(12.0),
                    ))
                    .clicked()
                {
                    self.settings_tab = idx;
                }
            }
        });
        ui.separator();
        ui.add_space(8.0);

        match self.settings_tab {
            0 => {
                // ── Appearance ────────────────────────────────────────
                settings_row(ui, pal, "Theme", |ui| {
                    for theme in [
                        Theme::System,
                        Theme::Dark,
                        Theme::Light,
                        Theme::HighContrast,
                    ] {
                        let active = self.config.theme == theme;
                        if ui
                            .add(egui::Button::selectable(
                                active,
                                RichText::new(theme.label())
                                    .color(if active { pal.accent } else { pal.subtext })
                                    .size(12.0),
                            ))
                            .clicked()
                        {
                            self.config.theme = theme;
                        }
                    }
                });
                settings_row(ui, pal, "Font size", |ui| {
                    ui.add(
                        egui::Slider::new(&mut self.config.font_size, 10.0_f32..=20.0)
                            .step_by(0.5)
                            .suffix(" pt"),
                    );
                    if ui.small_button("Reset").clicked() {
                        self.config.font_size = 13.0;
                    }
                });
                settings_row(ui, pal, "Custom CJK Font", |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.config.custom_font_path)
                                .desired_width(180.0)
                                .hint_text("Path to .ttf or .ttc file"),
                        );
                        if ui.small_button("Browse...").clicked() {
                            if let Some(p) = rfd::FileDialog::new()
                                .add_filter("Fonts", &["ttf", "ttc", "otf"])
                                .pick_file()
                            {
                                self.config.custom_font_path = p.to_string_lossy().to_string();
                            }
                        }
                        if !self.config.custom_font_path.is_empty()
                            && ui.small_button("Clear").clicked()
                        {
                            self.config.custom_font_path.clear();
                        }
                    });
                });
                settings_row(ui, pal, "Word wrap", |ui| {
                    ui.checkbox(&mut self.config.wrap_lines, "Wrap long lines");
                });
                settings_row(ui, pal, "Reduce motion", |ui| {
                    ui.checkbox(
                        &mut self.config.reduce_motion,
                        "Disable transitions & fades",
                    );
                });
            }
            1 => {
                // ── Search ────────────────────────────────────────────
                settings_row(ui, pal, "History limit", |ui| {
                    ui.add(egui::Slider::new(&mut self.config.history_limit, 1..=1000));
                });
                settings_row(ui, pal, "Search threads", |ui| {
                    ui.checkbox(&mut self.config.auto_threads, "Auto");
                    if !self.config.auto_threads {
                        ui.add(egui::Slider::new(&mut self.config.max_threads, 1..=16));
                    } else {
                        let n = std::thread::available_parallelism()
                            .map(|n| n.get())
                            .unwrap_or(4);
                        ui.label(
                            RichText::new(format!("({n} detected)"))
                                .color(pal.muted)
                                .size(11.0),
                        );
                    }
                });
                settings_row(ui, pal, "Max file size (MB)", |ui| {
                    ui.add(egui::Slider::new(
                        &mut self.config.max_file_size_mb,
                        1..=500,
                    ));
                });
                settings_row(ui, pal, "Limit result files", |ui| {
                    let mut limit_enabled = self.config.max_result_files != 0;
                    if ui.checkbox(&mut limit_enabled, "").changed() {
                        if limit_enabled {
                            self.config.max_result_files = 2000;
                        } else {
                            self.config.max_result_files = 0;
                        }
                    }
                    if limit_enabled {
                        ui.add(egui::Slider::new(
                            &mut self.config.max_result_files,
                            100..=10000,
                        ));
                    } else {
                        ui.label(RichText::new("Unlimited").color(pal.muted));
                    }
                });
                settings_row(ui, pal, "Backup on replace", |ui| {
                    ui.checkbox(&mut self.config.backup_before_replace, "Enable backups");
                });
                if self.config.backup_before_replace {
                    settings_row(ui, pal, "Backup directory", |ui| {
                        ui.horizontal(|ui| {
                            let input_w = (ui.available_width() - 32.0).max(200.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.config.backup_dir)
                                    .desired_width(input_w),
                            );
                            if ui
                                .add(egui::Button::new(icon_rt(icons::FOLDER, 13.0, pal.accent)))
                                .on_hover_text("Browse backup folder")
                                .clicked()
                            {
                                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                                    self.config.backup_dir = p.to_string_lossy().to_string();
                                }
                            }
                        });
                    });
                    settings_row(ui, pal, "Backup retention", |ui| {
                        ui.add(
                            egui::Slider::new(&mut self.config.backup_retention_days, 0..=90)
                                .suffix(" days")
                                .custom_formatter(|val, _| {
                                    if val == 0.0 {
                                        "Keep forever".to_string()
                                    } else {
                                        format!("{:.0}", val)
                                    }
                                }),
                        );
                    });
                }
                settings_row(ui, pal, "Respect .gitignore", |ui| {
                    ui.checkbox(&mut self.config.respect_gitignore, "Skip gitignored paths");
                });
                settings_row(ui, pal, "Default excludes", |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config.default_exclude_dirs)
                            .hint_text(
                                RichText::new(".git,target,node_modules")
                                    .color(pal.placeholder)
                                    .italics(),
                            )
                            .desired_width(f32::INFINITY),
                    );
                });
            }
            2 => {
                // ── Editor ────────────────────────────────────────────
                settings_row(ui, pal, "Command", |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.config.editor_command)
                            .hint_text(
                                RichText::new("zed  /  code -g  /  nvim")
                                    .color(pal.placeholder)
                                    .italics(),
                            )
                            .desired_width(f32::INFINITY),
                    );
                });
                settings_row(ui, pal, "Presets", |ui| {
                    for (label, cmd) in [
                        ("Zed", "zed"),
                        ("VS Code", "code -g"),
                        ("Neovim", "nvim"),
                        ("Vim", "vim"),
                    ] {
                        if ui.small_button(RichText::new(label).size(11.0)).clicked() {
                            self.config.editor_command = cmd.to_string();
                        }
                    }
                });
                settings_row(ui, pal, "Test", |ui| {
                    let editor_cmd = self.config.editor_command.clone();
                    let test_available = !editor_cmd.trim().is_empty();
                    ui.add_enabled_ui(test_available, |ui| {
                        if ui
                            .button(RichText::new("Open this file").size(11.0))
                            .clicked()
                        {
                            let self_path = std::path::Path::new(file!());
                            open_in_editor(self_path, Some(1), &editor_cmd);
                        }
                    });
                    ui.label(
                        RichText::new("(<command> <file>:<line>)")
                            .color(pal.muted)
                            .size(10.5),
                    );
                });
            }
            3 => {
                // ── Presets ───────────────────────────────────────────
                let last_hovered_idx = self.dnd_hovered_preset_idx;
                self.dnd_hovered_preset_idx = None;

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(
                            "Configure type preset shortcuts shown in the search toolbar",
                        )
                        .color(pal.muted)
                        .size(11.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button(RichText::new("Reset Defaults").color(pal.red))
                            .clicked()
                        {
                            self.config.presets = crate::config::default_presets();
                        }
                    });
                });
                ui.add_space(8.0);

                let mut to_remove = None;
                let mut swap_up = None;
                let mut swap_down = None;
                let mut move_to_top = None;
                let mut move_to_bottom = None;
                let mut dnd_move = None;

                ScrollArea::vertical()
                    .id_salt("presets_scroll")
                    .max_height(300.0)
                    .show(ui, |ui| {
                        egui::Frame::NONE
                            .inner_margin(Margin {
                                left: 4,
                                right: 18,
                                top: 4,
                                bottom: 4,
                            })
                            .show(ui, |ui| {
                                // Card inner_margin is left:8 + right:8 = 16px
                                let card_content_w = ui.available_width() - 16.0;
                                ui.vertical(|ui| {
                                    let presets_len = self.config.presets.len();
                                    for i in 0..presets_len {
                                        let is_editing = self.editing_preset_idx == Some(i);

                                        let is_this_row_dragged = ui.ctx().dragged_id()
                                            == Some(egui::Id::new(("preset_drag", i)));
                                        let is_hovered = last_hovered_idx == Some(i);

                                        let mut card_frame = egui::Frame::NONE
                                            .inner_margin(Margin {
                                                left: 8,
                                                right: 8,
                                                top: 6,
                                                bottom: 6,
                                            })
                                            .corner_radius(egui::CornerRadius::same(4));

                                        // Hovered: keep normal appearance; insert line is
                                        // drawn separately below the drop zone call.
                                        if is_this_row_dragged {
                                            card_frame = card_frame
                                                .fill(pal.bg_surface0)
                                                .stroke(egui::Stroke::new(1.0, pal.bg_surface1));
                                        } else {
                                            card_frame = card_frame
                                                .fill(pal.bg_mantle)
                                                .stroke(egui::Stroke::new(1.0, pal.bg_surface0));
                                        }

                                        let (inner, dropped_payload) = ui
                                            .dnd_drop_zone::<usize, _>(card_frame, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.set_min_width(card_content_w);
                                                    ui.spacing_mut().item_spacing =
                                                        Vec2::new(6.0, 0.0);
                                                    ui.spacing_mut().interact_size.y = 22.0;

                                                    // 1. Grabber handle
                                                    let grabber_color = if is_this_row_dragged {
                                                        pal.muted
                                                    } else {
                                                        pal.subtext
                                                    };
                                                    let _drag_response = ui.dnd_drag_source(
                                                        egui::Id::new(("preset_drag", i)),
                                                        i,
                                                        |ui| {
                                                            ui.add(
                                                                egui::Label::new(icon_rt(
                                                                    icons::GRABBER,
                                                                    13.0,
                                                                    grabber_color,
                                                                ))
                                                                .sense(egui::Sense::drag()),
                                                            )
                                                            .on_hover_text("Drag to reorder");
                                                        },
                                                    );

                                                    // 2. Enabled checkbox
                                                    let mut enabled =
                                                        self.config.presets[i].enabled;
                                                    if ui.checkbox(&mut enabled, "").changed() {
                                                        self.config.presets[i].enabled = enabled;
                                                    }

                                                    // 3. Name (fixed 120px)
                                                    if is_editing {
                                                        let mut name =
                                                            self.config.presets[i].name.clone();
                                                        let res = ui.add_sized(
                                                            [120.0, 22.0],
                                                            egui::TextEdit::singleline(&mut name)
                                                                .hint_text("Name"),
                                                        );
                                                        if res.changed() {
                                                            self.config.presets[i].name = name;
                                                        }
                                                        if res.lost_focus()
                                                            && ui.input(|i| {
                                                                i.key_pressed(egui::Key::Enter)
                                                            })
                                                        {
                                                            self.editing_preset_idx = None;
                                                        }
                                                    } else {
                                                        let name = &self.config.presets[i].name;
                                                        let text_color = if is_this_row_dragged {
                                                            pal.muted
                                                        } else {
                                                            pal.text
                                                        };
                                                        ui.allocate_ui_with_layout(
                                                            Vec2::new(120.0, 22.0),
                                                            egui::Layout::left_to_right(
                                                                egui::Align::Center,
                                                            ),
                                                            |ui| {
                                                                let pad =
                                                                    ui.spacing().button_padding.x;
                                                                ui.spacing_mut().item_spacing.x =
                                                                    0.0;
                                                                ui.add_space(pad);
                                                                ui.add(
                                                                    egui::Label::new(
                                                                        RichText::new(name)
                                                                            .color(text_color),
                                                                    )
                                                                    .truncate(),
                                                                );
                                                            },
                                                        );
                                                    }

                                                    // 4-7. Glob + actions pinned to right
                                                    let action_color = if is_this_row_dragged {
                                                        pal.muted
                                                    } else {
                                                        pal.subtext
                                                    };
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.spacing_mut().item_spacing =
                                                                Vec2::new(4.0, 0.0);

                                                            // 7. Delete (rightmost)
                                                            if preset_icon_btn(
                                                                ui,
                                                                pal,
                                                                icons::TRASH,
                                                                12.0,
                                                                pal.red,
                                                                "Delete preset",
                                                            )
                                                            .clicked()
                                                            {
                                                                to_remove = Some(i);
                                                            }

                                                            // 6. Reorder cluster
                                                            ui.horizontal(|ui| {
                                                                ui.spacing_mut().item_spacing =
                                                                    Vec2::new(2.0, 0.0);
                                                                if preset_icon_btn(
                                                                    ui,
                                                                    pal,
                                                                    icons::FOLD_UP,
                                                                    11.0,
                                                                    action_color,
                                                                    "Move to top",
                                                                )
                                                                .clicked()
                                                                {
                                                                    move_to_top = Some(i);
                                                                }
                                                                if preset_icon_btn(
                                                                    ui,
                                                                    pal,
                                                                    icons::CHEVRON_UP,
                                                                    11.0,
                                                                    action_color,
                                                                    "Move up",
                                                                )
                                                                .clicked()
                                                                {
                                                                    swap_up = Some(i);
                                                                }
                                                                if preset_icon_btn(
                                                                    ui,
                                                                    pal,
                                                                    icons::CHEVRON_DOWN,
                                                                    11.0,
                                                                    action_color,
                                                                    "Move down",
                                                                )
                                                                .clicked()
                                                                {
                                                                    swap_down = Some(i);
                                                                }
                                                                if preset_icon_btn(
                                                                    ui,
                                                                    pal,
                                                                    icons::FOLD_DOWN,
                                                                    11.0,
                                                                    action_color,
                                                                    "Move to bottom",
                                                                )
                                                                .clicked()
                                                                {
                                                                    move_to_bottom = Some(i);
                                                                }
                                                            });

                                                            // 5. Edit/Check button
                                                            if is_editing {
                                                                if preset_icon_btn(
                                                                    ui,
                                                                    pal,
                                                                    icons::CHECK,
                                                                    12.0,
                                                                    pal.green,
                                                                    "Save changes",
                                                                )
                                                                .clicked()
                                                                {
                                                                    self.editing_preset_idx = None;
                                                                }
                                                            } else if preset_icon_btn(
                                                                ui,
                                                                pal,
                                                                icons::EDIT,
                                                                12.0,
                                                                pal.accent,
                                                                "Edit preset",
                                                            )
                                                            .clicked()
                                                            {
                                                                self.editing_preset_idx = Some(i);
                                                            }

                                                            // 4. Glob (fills remaining, leftmost)
                                                            if is_editing {
                                                                let mut glob = self.config.presets
                                                                    [i]
                                                                    .glob
                                                                    .clone();
                                                                let glob_w =
                                                                    ui.available_width().max(0.0);
                                                                let res = ui.add_sized(
                                                                    [glob_w, 22.0],
                                                                    egui::TextEdit::singleline(
                                                                        &mut glob,
                                                                    )
                                                                    .hint_text("Globs (e.g. *.rs)"),
                                                                );
                                                                if res.changed() {
                                                                    self.config.presets[i].glob =
                                                                        glob;
                                                                }
                                                                if res.lost_focus()
                                                                    && ui.input(|i| {
                                                                        i.key_pressed(
                                                                            egui::Key::Enter,
                                                                        )
                                                                    })
                                                                {
                                                                    self.editing_preset_idx = None;
                                                                }
                                                            } else {
                                                                let glob =
                                                                    &self.config.presets[i].glob;
                                                                let glob_color =
                                                                    if is_this_row_dragged {
                                                                        pal.muted
                                                                    } else {
                                                                        pal.subtext
                                                                    };
                                                                let glob_w =
                                                                    ui.available_width().max(0.0);
                                                                ui.allocate_ui_with_layout(
                                                                    Vec2::new(glob_w, 22.0),
                                                                    egui::Layout::left_to_right(
                                                                        egui::Align::Center,
                                                                    ),
                                                                    |ui| {
                                                                        let pad = ui
                                                                            .spacing()
                                                                            .button_padding
                                                                            .x;
                                                                        ui.spacing_mut()
                                                                            .item_spacing
                                                                            .x = 0.0;
                                                                        ui.add_space(pad);
                                                                        ui.add(
                                                                            egui::Label::new(
                                                                                RichText::new(glob)
                                                                                    .color(
                                                                                        glob_color,
                                                                                    ),
                                                                            )
                                                                            .truncate(),
                                                                        );
                                                                    },
                                                                );
                                                            }
                                                        },
                                                    );
                                                });
                                            });

                                        // Set hover index for the next frame if dragged preset is hovered here
                                        if let Some(source_idx_arc) =
                                            inner.response.dnd_hover_payload::<usize>()
                                        {
                                            let source_idx = *source_idx_arc;
                                            if source_idx != i {
                                                self.dnd_hovered_preset_idx = Some(i);
                                            }
                                        }

                                        if let Some(source_idx_arc) = dropped_payload {
                                            let source_idx = *source_idx_arc;
                                            if source_idx != i {
                                                dnd_move = Some((source_idx, i));
                                            }
                                        }

                                        // Draw accent insert line at top of the hovered card
                                        if is_hovered {
                                            let r = inner.response.rect;
                                            ui.painter().hline(
                                                r.x_range(),
                                                r.top(),
                                                egui::Stroke::new(2.0, pal.accent),
                                            );
                                        }

                                        ui.add_space(4.0);
                                    }
                                });
                            });
                    });

                // Draw a floating preview for the dragged preset (floating drag avatar)
                if let Some(mouse_pos) = ui.ctx().pointer_latest_pos() {
                    if let Some(dragged_id) = ui.ctx().dragged_id() {
                        let presets_len = self.config.presets.len();
                        for i in 0..presets_len {
                            if dragged_id == egui::Id::new(("preset_drag", i)) {
                                egui::Area::new(egui::Id::new("preset_drag_preview"))
                                    .fixed_pos(mouse_pos + Vec2::new(8.0, 4.0))
                                    .order(egui::Order::Tooltip)
                                    .interactable(false)
                                    .show(ui.ctx(), |ui| {
                                        let name = &self.config.presets[i].name;
                                        let glob = &self.config.presets[i].glob;
                                        egui::Frame::window(ui.style())
                                            .fill(pal.bg_surface0)
                                            .stroke(egui::Stroke::new(1.0, pal.accent))
                                            .corner_radius(egui::CornerRadius::same(4))
                                            .inner_margin(Margin {
                                                left: 8,
                                                right: 8,
                                                top: 4,
                                                bottom: 4,
                                            })
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.add(egui::Label::new(icon_rt(
                                                        icons::GRABBER,
                                                        13.0,
                                                        pal.accent,
                                                    )));
                                                    ui.label(
                                                        RichText::new(name)
                                                            .color(pal.text)
                                                            .strong(),
                                                    );
                                                    ui.label(
                                                        RichText::new(glob).color(pal.subtext),
                                                    );
                                                });
                                            });
                                    });
                                break;
                            }
                        }
                    }
                }

                if let Some((source_idx, target_idx)) = dnd_move {
                    if source_idx < self.config.presets.len()
                        && target_idx < self.config.presets.len()
                    {
                        let preset = self.config.presets.remove(source_idx);
                        self.config.presets.insert(target_idx, preset);
                        if self.editing_preset_idx == Some(source_idx) {
                            self.editing_preset_idx = Some(target_idx);
                        } else if let Some(edit_idx) = self.editing_preset_idx {
                            if source_idx < edit_idx && target_idx >= edit_idx {
                                self.editing_preset_idx = Some(edit_idx - 1);
                            } else if source_idx > edit_idx && target_idx <= edit_idx {
                                self.editing_preset_idx = Some(edit_idx + 1);
                            }
                        }
                    }
                }
                if let Some(idx) = move_to_top {
                    if idx > 0 {
                        let preset = self.config.presets.remove(idx);
                        self.config.presets.insert(0, preset);
                        if self.editing_preset_idx == Some(idx) {
                            self.editing_preset_idx = Some(0);
                        } else if let Some(edit_idx) = self.editing_preset_idx {
                            if edit_idx < idx {
                                self.editing_preset_idx = Some(edit_idx + 1);
                            }
                        }
                    }
                }
                if let Some(idx) = swap_up {
                    if idx > 0 {
                        self.config.presets.swap(idx, idx - 1);
                        if self.editing_preset_idx == Some(idx) {
                            self.editing_preset_idx = Some(idx - 1);
                        } else if self.editing_preset_idx == Some(idx - 1) {
                            self.editing_preset_idx = Some(idx);
                        }
                    }
                }
                if let Some(idx) = swap_down {
                    if idx < self.config.presets.len() - 1 {
                        self.config.presets.swap(idx, idx + 1);
                        if self.editing_preset_idx == Some(idx) {
                            self.editing_preset_idx = Some(idx + 1);
                        } else if self.editing_preset_idx == Some(idx + 1) {
                            self.editing_preset_idx = Some(idx);
                        }
                    }
                }
                if let Some(idx) = move_to_bottom {
                    let last_idx = self.config.presets.len() - 1;
                    if idx < last_idx {
                        let preset = self.config.presets.remove(idx);
                        self.config.presets.push(preset);
                        if self.editing_preset_idx == Some(idx) {
                            self.editing_preset_idx = Some(last_idx);
                        } else if let Some(edit_idx) = self.editing_preset_idx {
                            if edit_idx > idx {
                                self.editing_preset_idx = Some(edit_idx - 1);
                            }
                        }
                    }
                }
                if let Some(idx) = to_remove {
                    self.config.presets.remove(idx);
                    if self.editing_preset_idx == Some(idx) {
                        self.editing_preset_idx = None;
                    } else if let Some(edit_idx) = self.editing_preset_idx {
                        if edit_idx > idx {
                            self.editing_preset_idx = Some(edit_idx - 1);
                        }
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Add New Preset")
                        .color(pal.accent)
                        .strong()
                        .size(12.0),
                );
                let new_glob_w = (ui.available_width() - 210.0).max(200.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.preset_new_name)
                            .hint_text("Name (e.g. HTML)")
                            .desired_width(120.0),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.preset_new_glob)
                            .hint_text("Glob (e.g. *.html,*.htm)")
                            .desired_width(new_glob_w),
                    );
                    if ui.button(RichText::new("Add").size(12.0)).clicked() {
                        let name = self.preset_new_name.trim().to_string();
                        let glob = self.preset_new_glob.trim().to_string();
                        if !name.is_empty() && !glob.is_empty() {
                            self.config.presets.push(crate::config::Preset {
                                name,
                                glob,
                                enabled: true,
                            });
                            self.preset_new_name.clear();
                            self.preset_new_glob.clear();
                        }
                    }
                });
            }
            4 => {
                // ── Export ────────────────────────────────────────────
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(
                            "Configure formatting for search results copied to clipboard",
                        )
                        .color(pal.muted)
                        .size(11.0),
                    );
                });
                ui.add_space(8.0);

                // Preset selector
                settings_row(ui, pal, "Preset Format", |ui| {
                    ui.horizontal(|ui| {
                        for (p, label) in [
                            (crate::config::ExportPreset::Standard, "Standard (%f:%l)"),
                            (
                                crate::config::ExportPreset::IdeCompatible,
                                "IDE/Editor (%f:%n:%l)",
                            ),
                            (crate::config::ExportPreset::ModernTree, "Modern Grouped"),
                            (crate::config::ExportPreset::Custom, "Custom"),
                        ] {
                            if ui
                                .selectable_label(self.config.export_preset == p, label)
                                .clicked()
                            {
                                self.config.export_preset = p;
                                match p {
                                    crate::config::ExportPreset::Standard => {
                                        self.config.export_output_mode =
                                            crate::config::ExportOutputMode::Flat;
                                        self.config.export_line_format = "%f:%l".to_string();
                                    }
                                    crate::config::ExportPreset::IdeCompatible => {
                                        self.config.export_output_mode =
                                            crate::config::ExportOutputMode::Flat;
                                        self.config.export_line_format = "%f:%n:%l".to_string();
                                    }
                                    crate::config::ExportPreset::ModernTree => {
                                        self.config.export_output_mode =
                                            crate::config::ExportOutputMode::Grouped;
                                        self.config.export_file_header_format = "%f".to_string();
                                        self.config.export_line_format = "%n: %l".to_string();
                                    }
                                    crate::config::ExportPreset::Custom => {}
                                }
                            }
                        }
                    });
                });

                // Output Mode (only editable in Custom preset)
                let is_custom = self.config.export_preset == crate::config::ExportPreset::Custom;
                settings_row(ui, pal, "Output Structure", |ui| {
                    ui.add_enabled_ui(is_custom, |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(
                                    self.config.export_output_mode
                                        == crate::config::ExportOutputMode::Flat,
                                    "Flat (row-by-row)",
                                )
                                .clicked()
                            {
                                self.config.export_output_mode =
                                    crate::config::ExportOutputMode::Flat;
                            }
                            if ui
                                .selectable_label(
                                    self.config.export_output_mode
                                        == crate::config::ExportOutputMode::Grouped,
                                    "Grouped (by file)",
                                )
                                .clicked()
                            {
                                self.config.export_output_mode =
                                    crate::config::ExportOutputMode::Grouped;
                            }
                        });
                    });
                });

                // Custom Header Format (only for Grouped mode)
                if self.config.export_output_mode == crate::config::ExportOutputMode::Grouped {
                    settings_row(ui, pal, "File Heading Format", |ui| {
                        let mut header = self.config.export_file_header_format.clone();
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut header)
                                .desired_width(240.0)
                                .hint_text("e.g. ### %f"),
                        );
                        if resp.changed() {
                            self.config.export_file_header_format = header;
                            self.config.export_preset = crate::config::ExportPreset::Custom;
                        }
                    });
                }

                // Custom Line Format
                settings_row(ui, pal, "Match Line Format", |ui| {
                    let mut line = self.config.export_line_format.clone();
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut line)
                            .desired_width(240.0)
                            .hint_text("e.g. %f:%n:%l"),
                    );
                    if resp.changed() {
                        self.config.export_line_format = line;
                        self.config.export_preset = crate::config::ExportPreset::Custom;
                    }
                });

                // Omit Single File name checkbox
                settings_row(ui, pal, "Omit file name", |ui| {
                    ui.checkbox(
                        &mut self.config.export_omit_single_file_name,
                        "Omit path header when only one file is matched",
                    );
                });

                // Custom Global Header Format
                settings_row(ui, pal, "Global Header", |ui| {
                    ui.checkbox(
                        &mut self.config.export_header_enabled,
                        "Include metadata header at start of clipboard",
                    );
                });
                if self.config.export_header_enabled {
                    settings_row(ui, pal, "Header Template", |ui| {
                        let mut gheader = self.config.export_header_format.clone();
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut gheader)
                                .desired_width(240.0)
                                .hint_text("e.g. Search: %q (%t)%N---"),
                        );
                        if resp.changed() {
                            self.config.export_header_format = gheader;
                            self.config.export_preset = crate::config::ExportPreset::Custom;
                        }
                    });
                }

                // Placeholders help panel
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Placeholders Reference")
                        .strong()
                        .color(pal.accent)
                        .size(12.0),
                );
                ui.add_space(4.0);
                egui::Grid::new("placeholder_help_grid")
                    .num_columns(2)
                    .spacing([16.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("%f").strong().color(pal.text));
                        ui.label("File path (relative to search root)");
                        ui.end_row();
                        ui.label(RichText::new("%n").strong().color(pal.text));
                        ui.label("Line number");
                        ui.end_row();
                        ui.label(RichText::new("%l").strong().color(pal.text));
                        ui.label("Match line content");
                        ui.end_row();
                        ui.label(RichText::new("%m").strong().color(pal.text));
                        ui.label("Matched keyword/phrase");
                        ui.end_row();
                        ui.label(RichText::new("%N").strong().color(pal.text));
                        ui.label("Newline character");
                        ui.end_row();
                        ui.label(RichText::new("%T").strong().color(pal.text));
                        ui.label("Tab space");
                        ui.end_row();
                        ui.label(RichText::new("%q").strong().color(pal.text));
                        ui.label("Search query pattern (Global Header)");
                        ui.end_row();
                        ui.label(RichText::new("%d").strong().color(pal.text));
                        ui.label("Search directory (Global Header)");
                        ui.end_row();
                        ui.label(RichText::new("%g").strong().color(pal.text));
                        ui.label("Glob pattern (Global Header)");
                        ui.end_row();
                        ui.label(RichText::new("%x").strong().color(pal.text));
                        ui.label("Exclude pattern (Global Header)");
                        ui.end_row();
                        ui.label(RichText::new("%t").strong().color(pal.text));
                        ui.label("Timestamp of copy (Global Header)");
                        ui.end_row();
                    });
            }
            _ => {}
        }

        if self.config != old_config {
            self.history.set_limit(self.config.history_limit);
            let _ = self.config.save();
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("Close").color(pal.bg_mantle).size(13.0))
                        .fill(pal.accent),
                )
                .clicked()
            {
                self.close_settings_tab();
            }
        });
    }

    fn show_replace_preview_window(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        let Some((params, entries)) = &self.replace_preview else {
            return;
        };
        let params = params.clone();
        let entries = entries.clone();

        let total_files = entries.len();
        let total_changed_lines: usize = entries
            .iter()
            .map(|(_, orig, prev)| {
                let ol: Vec<&str> = orig.lines().collect();
                let pl: Vec<&str> = prev.lines().collect();
                (0..ol.len().max(pl.len()))
                    .filter(|&i| {
                        ol.get(i).copied().unwrap_or("") != pl.get(i).copied().unwrap_or("")
                    })
                    .count()
            })
            .sum();

        // Header summary
        let scope_note = if params.replace_scope == crate::models::ReplaceScope::All {
            "  (All scope — up to 20 files shown)".to_string()
        } else {
            String::new()
        };
        ui.label(
            RichText::new(format!(
                "{} file(s), {} line(s) changed{}",
                total_files, total_changed_lines, scope_note
            ))
            .color(pal.subtext)
            .size(11.0),
        );
        ui.add_space(4.0);

        let regex = build_regex(&params).ok();

        ScrollArea::both().id_salt("replace_diff").show(ui, |ui| {
            for (file_idx, (path, original, preview)) in entries.iter().enumerate() {
                if file_idx > 0 {
                    ui.add_space(8.0);
                }

                // File header
                ui.label(
                    RichText::new(path.to_string_lossy().as_ref())
                        .monospace()
                        .color(pal.accent)
                        .size(11.5),
                );
                ui.painter().hline(
                    ui.available_rect_before_wrap().x_range(),
                    ui.cursor().top(),
                    Stroke::new(1.0, pal.bg_surface0),
                );

                let orig_lines: Vec<&str> = original.lines().collect();
                let prev_lines: Vec<&str> = preview.lines().collect();
                let max_len = orig_lines.len().max(prev_lines.len());

                let changed: Vec<bool> = (0..max_len)
                    .map(|i| {
                        orig_lines.get(i).copied().unwrap_or("")
                            != prev_lines.get(i).copied().unwrap_or("")
                    })
                    .collect();

                let mut visible = vec![false; max_len];
                for (i, &is_changed) in changed.iter().enumerate() {
                    if is_changed {
                        for v in visible
                            .iter_mut()
                            .take((i + 3).min(max_len))
                            .skip(i.saturating_sub(2))
                        {
                            *v = true;
                        }
                    }
                }

                let file_changes = changed.iter().filter(|&&c| c).count();
                if file_changes == 0 {
                    ui.label(RichText::new("  (no changes)").color(pal.muted).size(11.0));
                    continue;
                }

                let mut last_visible = false;
                for i in 0..max_len {
                    if !visible[i] {
                        if last_visible {
                            ui.label(RichText::new("  …").color(pal.muted).monospace().size(11.0));
                        }
                        last_visible = false;
                        continue;
                    }
                    last_visible = true;
                    let line_no = i + 1;
                    if changed[i] {
                        let prefix = format!("- {:4}  ", line_no);
                        let orig_line = orig_lines.get(i).copied().unwrap_or("");
                        let new_line = prev_lines.get(i).copied().unwrap_or("");

                        // Intra-line: find match ranges in original line for - side
                        let match_ranges: Vec<(usize, usize)> = regex
                            .as_ref()
                            .map(|re| {
                                re.find_iter(orig_line)
                                    .map(|m| (m.start(), m.end()))
                                    .collect()
                            })
                            .unwrap_or_default();

                        // - line: light-red background, match ranges in strong red
                        {
                            let bg_red = Color32::from_rgba_unmultiplied(
                                pal.red.r(),
                                pal.red.g(),
                                pal.red.b(),
                                30,
                            );
                            let hi_red = Color32::from_rgba_unmultiplied(
                                pal.red.r(),
                                pal.red.g(),
                                pal.red.b(),
                                80,
                            );
                            let mut job = LayoutJob::default();
                            let base_fmt = TextFormat {
                                font_id: FontId::monospace(12.0),
                                color: pal.red,
                                background: bg_red,
                                ..Default::default()
                            };
                            let hi_fmt = TextFormat {
                                font_id: FontId::monospace(12.0),
                                color: pal.red,
                                background: hi_red,
                                ..Default::default()
                            };
                            let prefix_fmt = TextFormat {
                                background: bg_red,
                                ..base_fmt.clone()
                            };
                            job.append(&prefix, 0.0, prefix_fmt);
                            // render orig_line with match ranges highlighted
                            let mut pos = 0usize;
                            for (s, e) in &match_ranges {
                                let s = (*s).min(orig_line.len());
                                let e = (*e).min(orig_line.len());
                                if pos < s {
                                    job.append(&orig_line[pos..s], 0.0, base_fmt.clone());
                                }
                                if s < e {
                                    job.append(&orig_line[s..e], 0.0, hi_fmt.clone());
                                }
                                pos = e;
                            }
                            if pos < orig_line.len() {
                                job.append(&orig_line[pos..], 0.0, base_fmt);
                            }
                            ui.add(egui::Label::new(job));
                        }

                        // + line: light-green background, changed range highlighted
                        {
                            let bg_green = Color32::from_rgba_unmultiplied(
                                pal.green.r(),
                                pal.green.g(),
                                pal.green.b(),
                                30,
                            );
                            let hi_green = Color32::from_rgba_unmultiplied(
                                pal.green.r(),
                                pal.green.g(),
                                pal.green.b(),
                                80,
                            );
                            let new_prefix = format!("+ {:4}  ", line_no);
                            let mut job = LayoutJob::default();
                            let base_fmt = TextFormat {
                                font_id: FontId::monospace(12.0),
                                color: pal.green,
                                background: bg_green,
                                ..Default::default()
                            };
                            let hi_fmt = TextFormat {
                                font_id: FontId::monospace(12.0),
                                color: pal.green,
                                background: hi_green,
                                ..Default::default()
                            };
                            let prefix_fmt = TextFormat {
                                background: bg_green,
                                ..base_fmt.clone()
                            };
                            job.append(&new_prefix, 0.0, prefix_fmt);
                            // Compute changed region via common prefix/suffix
                            let pfx = common_prefix_len(orig_line, new_line);
                            let sfx = common_suffix_len(orig_line, new_line, pfx);
                            let ch_start = pfx;
                            let ch_end = new_line.len().saturating_sub(sfx);
                            if ch_start < ch_end {
                                if ch_start > 0 {
                                    job.append(&new_line[..ch_start], 0.0, base_fmt.clone());
                                }
                                job.append(&new_line[ch_start..ch_end], 0.0, hi_fmt.clone());
                                if ch_end < new_line.len() {
                                    job.append(&new_line[ch_end..], 0.0, base_fmt);
                                }
                            } else {
                                job.append(new_line, 0.0, base_fmt);
                            }
                            ui.add(egui::Label::new(job));
                        }
                    } else {
                        let line = orig_lines.get(i).copied().unwrap_or("");
                        ui.label(
                            RichText::new(format!("  {:4}  {}", line_no, line))
                                .monospace()
                                .color(pal.subtext)
                                .size(11.5),
                        );
                    }
                }
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("Apply All").color(pal.bg_mantle))
                        .fill(pal.accent),
                )
                .on_hover_text("Write all previewed replacements to disk")
                .clicked()
            {
                let mut ok = 0usize;
                let mut err = 0usize;
                for (path, _, preview) in &entries {
                    if std::fs::write(path, preview).is_ok() {
                        ok += 1;
                    } else {
                        err += 1;
                    }
                }
                if err == 0 {
                    self.status_msg = format!("Replaced {} file(s)", ok);
                } else {
                    self.status_msg = format!("Replaced {}, {} write error(s)", ok, err);
                }
                self.replace_preview = None;
            }
            if ui
                .button(RichText::new("Cancel").color(pal.subtext))
                .clicked()
            {
                self.replace_preview = None;
            }
        });
    }

    fn show_replace_confirm_window(&mut self, ui: &mut Ui) {
        let pal = self.pal;
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "Are you sure you want to replace {} occurrences in {} files?",
                    self.replace_confirm_matches, self.replace_confirm_files
                ))
                .size(13.0)
                .strong(),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new("This operation will modify the files on disk and cannot be undone.")
                    .color(pal.red)
                    .size(12.0),
            );
            if self.config.backup_before_replace {
                ui.label(
                    RichText::new(format!(
                        "Backups will be saved to: {}",
                        self.config.backup_dir
                    ))
                    .color(pal.green)
                    .size(11.0),
                );
            } else {
                ui.label(
                    RichText::new(
                        "WARNING: Backup is currently DISABLED. Files will be modified directly.",
                    )
                    .color(pal.yellow)
                    .size(11.0)
                    .strong(),
                );
            }
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Confirm Replace")
                                .color(pal.bg_mantle)
                                .size(13.0),
                        )
                        .fill(pal.red),
                    )
                    .clicked()
                {
                    self.execute_replace();
                }
                if ui
                    .button(RichText::new("Cancel").color(pal.subtext).size(13.0))
                    .clicked()
                {
                    self.replace_confirm_snapshot = None;
                    self.show_replace_confirm = false;
                }
            });
        });
    }

    fn show_shortcuts_window(&self, ui: &mut Ui) {
        let pal = self.pal;

        fn row(ui: &mut Ui, pal: Pal, keys: &str, desc: &str) {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [160.0, 18.0],
                    egui::Label::new(RichText::new(keys).monospace().size(12.0).color(pal.accent)),
                );
                ui.label(RichText::new(desc).size(12.0).color(pal.subtext));
            });
        }

        ui.add_space(4.0);
        ui.label(RichText::new("Search").size(11.0).color(pal.muted).strong());
        ui.separator();
        row(ui, pal, "Ctrl/Cmd+F", "Focus search pattern");
        row(ui, pal, "Enter", "Run search (from pattern/dir field)");
        row(ui, pal, "Ctrl/Cmd+T", "New tab");
        row(ui, pal, "Ctrl/Cmd+K", "Open command palette");
        row(ui, pal, "Esc", "Close palette / dialog / cancel search");

        ui.add_space(8.0);
        ui.label(
            RichText::new("Navigation")
                .size(11.0)
                .color(pal.muted)
                .strong(),
        );
        ui.separator();
        row(ui, pal, "F3  /  Ctrl/Cmd+G", "Next match");
        row(ui, pal, "Shift+F3  /  Shift+Ctrl/Cmd+G", "Previous match");
        row(
            ui,
            pal,
            "Up / Down  (file list)",
            "Select previous / next file",
        );
        row(
            ui,
            pal,
            "Enter  (file list)",
            "Open selected file in editor",
        );

        ui.add_space(8.0);
        ui.label(
            RichText::new("Input fields")
                .size(11.0)
                .color(pal.muted)
                .strong(),
        );
        ui.separator();
        row(
            ui,
            pal,
            "Tab",
            "Focus next field (Dir > Pattern > Inc > Exc)",
        );
        row(ui, pal, "Shift+Tab", "Focus previous field");

        ui.add_space(8.0);
        ui.label(
            RichText::new("Content")
                .size(11.0)
                .color(pal.muted)
                .strong(),
        );
        ui.separator();
        row(
            ui,
            pal,
            "Click line",
            "Copy line to clipboard / set current match",
        );
        row(
            ui,
            pal,
            "Click line number",
            "Open file at that line in editor",
        );
        row(ui, pal, "Double-click file", "Open file in editor");
        ui.add_space(4.0);
    }
}

// ── File row ──────────────────────────────────────────────────────────────────
struct FileRowResp {
    clicked: bool,
    double_clicked: bool,
}

fn file_row(
    ui: &mut Ui,
    pal: Pal,
    label: &str,
    selected: bool,
    match_count: usize,
    hover_text: Option<&str>,
) -> FileRowResp {
    let mut clicked = false;
    let mut double_clicked = false;

    let text_color = if selected { pal.accent } else { pal.subtext };

    let resp = egui::Frame::NONE
        .fill(Color32::TRANSPARENT)
        .inner_margin(Margin {
            left: 4,
            right: 4,
            top: 2,
            bottom: 2,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                // Reserve fixed space for right cluster (match count only)
                // so the filename never overlaps them.
                let right_reserve = 35.0_f32;
                let name_w = (ui.available_width() - right_reserve).max(40.0);
                let row_h = ui.spacing().interact_size.y;
                let row = ui
                    .allocate_ui_with_layout(
                        Vec2::new(name_w, row_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(label)
                                        .color(text_color)
                                        .monospace()
                                        .size(12.0),
                                )
                                .sense(egui::Sense::click())
                                .wrap(),
                            )
                        },
                    )
                    .inner;
                if row.clicked() {
                    clicked = true;
                }
                if row.double_clicked() {
                    double_clicked = true;
                }

                ui.allocate_ui_with_layout(
                    Vec2::new(right_reserve, row_h),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        ui.label(
                            RichText::new(match_count.to_string())
                                .color(pal.muted)
                                .size(10.5),
                        );
                    },
                );
            });
        });

    let mut response = resp.response;
    if let Some(hover) = hover_text {
        response = response.on_hover_text(hover);
    }

    // Accent left bar for selected state (2px, drawn after layout)
    if selected {
        let r = response.rect;
        ui.painter().rect_filled(
            egui::Rect::from_min_size(r.min, Vec2::new(2.0, r.height())),
            CornerRadius::same(1),
            pal.accent,
        );
    }

    FileRowResp {
        clicked,
        double_clicked,
    }
}

// ── Flat tree items (virtual-scroll tree view) ────────────────────────────────
enum FlatTreeItem {
    Dir {
        name: String,
        path: PathBuf,
        rel_path: String,
        indent: u8,
        id: egui::Id,
        is_open: bool,
    },
    File {
        name: String,
        path: PathBuf,
        rel_path: String,
        match_count: usize,
        indent: u8,
    },
}

/// Pre-flattens the file tree into a `Vec<FlatTreeItem>` that represents only
/// the currently-visible rows (respecting each directory's open/closed state).
/// Open/closed state is read from egui's persistent memory using path-based IDs,
/// enabling O(visible) rendering via `show_rows`.
fn build_flat_tree(
    entries: &[(PathBuf, usize)],
    base: &Path,
    ctx: &egui::Context,
) -> Vec<FlatTreeItem> {
    use std::collections::BTreeMap;
    struct Node {
        dirs: BTreeMap<String, Node>,
        files: Vec<(PathBuf, usize)>,
    }
    impl Node {
        fn new() -> Self {
            Self {
                dirs: BTreeMap::new(),
                files: Vec::new(),
            }
        }
        fn insert(&mut self, rel: &Path, full: &Path, matches: usize) {
            let mut comps = rel.components();
            if let Some(first) = comps.next() {
                let name = first.as_os_str().to_string_lossy().to_string();
                let rest: &Path = comps.as_path();
                if rest == Path::new("") {
                    self.files.push((full.to_path_buf(), matches));
                } else {
                    self.dirs
                        .entry(name)
                        .or_insert_with(Node::new)
                        .insert(rest, full, matches);
                }
            }
        }
    }

    fn flatten(
        node: &Node,
        base: &Path,
        cur: &Path,
        ctx: &egui::Context,
        depth: u8,
        out: &mut Vec<FlatTreeItem>,
    ) {
        for (dir_name, child) in &node.dirs {
            let child_path = cur.join(dir_name);
            let id = egui::Id::new(&child_path);
            let state =
                egui::collapsing_header::CollapsingState::load_with_default_open(ctx, id, true);
            let is_open = state.is_open();
            let rel_path = child_path
                .strip_prefix(base)
                .unwrap_or(&child_path)
                .to_string_lossy()
                .to_string();
            out.push(FlatTreeItem::Dir {
                name: dir_name.clone(),
                path: child_path.clone(),
                rel_path,
                indent: depth,
                id,
                is_open,
            });
            if is_open {
                flatten(child, base, &child_path, ctx, depth + 1, out);
            }
        }
        for (path, count) in &node.files {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let rel_path = path
                .strip_prefix(base)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            out.push(FlatTreeItem::File {
                name,
                path: path.clone(),
                rel_path,
                match_count: *count,
                indent: depth,
            });
        }
    }

    let mut root = Node::new();
    for (path, count) in entries {
        let rel = path.strip_prefix(base).unwrap_or(path);
        root.insert(rel, path, *count);
    }
    let mut items = Vec::with_capacity(entries.len());
    flatten(&root, base, base, ctx, 0, &mut items);
    items
}

// ── Highlighted text ──────────────────────────────────────────────────────────
fn build_context_line(line: &str, pal: Pal, wrap_width: Option<f32>) -> LayoutJob {
    let mut job = LayoutJob::default();
    if let Some(w) = wrap_width {
        job.wrap.max_width = w;
        job.wrap.break_anywhere = true;
    }
    let normal = TextFormat {
        font_id: FontId::monospace(12.5),
        color: pal.muted,
        ..Default::default()
    };
    job.append(line, 0.0, normal);
    job
}

fn build_highlighted_line(
    line: &str,
    ranges: &[crate::models::MatchRange],
    pal: Pal,
    wrap_width: Option<f32>,
) -> LayoutJob {
    // In no-wrap mode, trim lines around the match so the match is always
    // visible without horizontal scrolling (BL-65). Threshold is low enough
    // to fit in a typical panel width (~150 chars ≈ 1080px at 7.2px/char).
    const LONG_LINE_THRESHOLD: usize = 150;
    if wrap_width.is_none() && !ranges.is_empty() && line.chars().count() > LONG_LINE_THRESHOLD {
        return build_highlighted_line_long(line, ranges, pal);
    }

    let mut job = LayoutJob::default();
    if let Some(w) = wrap_width {
        job.wrap.max_width = w;
        job.wrap.break_anywhere = true;
    }
    let normal = TextFormat {
        font_id: FontId::monospace(12.5),
        color: pal.text,
        ..Default::default()
    };
    let hi = TextFormat {
        font_id: FontId::monospace(12.5),
        color: pal.match_text,
        background: pal.match_bg,
        ..Default::default()
    };

    let mut pos = 0usize;
    for r in ranges {
        let start = floor_char_boundary(line, r.start.min(line.len()));
        let end = ceil_char_boundary(line, r.end.min(line.len()));
        if pos < start {
            job.append(&line[pos..start], 0.0, normal.clone());
        }
        if start < end {
            job.append(&line[start..end], 0.0, hi.clone());
        }
        pos = end;
    }
    if pos < line.len() {
        job.append(&line[pos..], 0.0, normal);
    }
    job
}

/// Truncated rendering for long lines in no-wrap mode.
/// Shows up to CTX_CHARS of context on each side of the match span,
/// with "..." ellipsis where text is omitted.
fn build_highlighted_line_long(
    line: &str,
    ranges: &[crate::models::MatchRange],
    pal: Pal,
) -> LayoutJob {
    const CTX_CHARS: usize = 60;

    let first_start = floor_char_boundary(line, ranges[0].start.min(line.len()));
    let last_end = ceil_char_boundary(line, ranges[ranges.len() - 1].end.min(line.len()));

    // Compute context window as byte offsets, walking by chars.
    let chars_before = line[..first_start].chars().count();
    let window_start_byte = if chars_before > CTX_CHARS {
        let skip = chars_before - CTX_CHARS;
        line.char_indices().nth(skip).map(|(i, _)| i).unwrap_or(0)
    } else {
        0
    };

    let chars_after = line[last_end..].chars().count();
    let window_end_byte = if chars_after > CTX_CHARS {
        let total = line.chars().count();
        let keep_up_to = total - (chars_after - CTX_CHARS);
        line.char_indices()
            .nth(keep_up_to)
            .map(|(i, _)| i)
            .unwrap_or(line.len())
    } else {
        line.len()
    };

    let show_prefix = window_start_byte > 0;
    let show_suffix = window_end_byte < line.len();
    let visible = &line[window_start_byte..window_end_byte];

    // Adjust ranges into visible-slice byte coordinates.
    let adj_ranges: Vec<_> = ranges
        .iter()
        .filter_map(|r| {
            let s = r.start.min(line.len());
            let e = r.end.min(line.len());
            if e <= window_start_byte || s >= window_end_byte {
                return None;
            }
            let adj_s = s.saturating_sub(window_start_byte);
            let adj_e = (e - window_start_byte).min(visible.len());
            Some((adj_s, adj_e))
        })
        .collect();

    let mut job = LayoutJob::default();
    let normal = TextFormat {
        font_id: FontId::monospace(12.5),
        color: pal.text,
        ..Default::default()
    };
    let ellipsis = TextFormat {
        font_id: FontId::monospace(12.5),
        color: pal.muted,
        ..Default::default()
    };
    let hi = TextFormat {
        font_id: FontId::monospace(12.5),
        color: pal.match_text,
        background: pal.match_bg,
        ..Default::default()
    };

    if show_prefix {
        job.append("...", 0.0, ellipsis.clone());
    }

    let mut pos = 0usize;
    for (s, e) in &adj_ranges {
        let start = floor_char_boundary(visible, *s);
        let end = ceil_char_boundary(visible, *e);
        if pos < start {
            job.append(&visible[pos..start], 0.0, normal.clone());
        }
        if start < end {
            job.append(&visible[start..end], 0.0, hi.clone());
        }
        pos = end;
    }
    if pos < visible.len() {
        job.append(&visible[pos..], 0.0, normal);
    }

    if show_suffix {
        job.append("...", 0.0, ellipsis);
    }

    job
}

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}
fn ceil_char_boundary(s: &str, mut i: usize) -> usize {
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Byte length of the longest common UTF-8 prefix of two strings.
fn common_prefix_len(a: &str, b: &str) -> usize {
    let n = a.len().min(b.len());
    let mut i = 0;
    while i < n && a.as_bytes()[i] == b.as_bytes()[i] {
        i += 1;
    }
    floor_char_boundary(a, i)
}

/// Byte length of the longest common UTF-8 suffix of two strings, not overlapping the prefix.
fn common_suffix_len(a: &str, b: &str, prefix: usize) -> usize {
    let a_tail = &a[prefix..];
    let b_tail = &b[prefix..];
    let n = a_tail.len().min(b_tail.len());
    let mut i = 0;
    while i < n
        && a_tail.as_bytes()[a_tail.len() - 1 - i] == b_tail.as_bytes()[b_tail.len() - 1 - i]
    {
        i += 1;
    }
    // Walk back to a char boundary in a_tail
    while i > 0 && !a_tail.is_char_boundary(a_tail.len() - i) {
        i -= 1;
    }
    i
}

// ── Theme & font ──────────────────────────────────────────────────────────────
fn apply_theme(ctx: &egui::Context, pal: Pal, tok: Tok) {
    let mut v = if pal.is_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    v.panel_fill = pal.bg_base;
    v.window_fill = pal.bg_base;
    v.faint_bg_color = pal.bg_mantle;
    v.extreme_bg_color = pal.bg_mantle;
    v.code_bg_color = pal.bg_surface0;
    v.window_stroke = Stroke::new(1.0, pal.bg_surface0);
    v.window_corner_radius = egui::CornerRadius::same(tok.r_lg as u8);

    v.widgets.noninteractive.bg_fill = pal.bg_base;
    v.widgets.noninteractive.weak_bg_fill = pal.bg_mantle;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, pal.bg_surface0);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, pal.subtext);
    v.widgets.noninteractive.corner_radius = egui::CornerRadius::same(tok.r_md as u8);

    v.widgets.inactive.bg_fill = pal.bg_surface0;
    v.widgets.inactive.weak_bg_fill = pal.bg_surface0;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, pal.bg_surface1);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, pal.text);
    v.widgets.inactive.corner_radius = egui::CornerRadius::same(tok.r_md as u8);

    v.widgets.hovered.bg_fill = pal.bg_surface1;
    v.widgets.hovered.weak_bg_fill = pal.bg_surface1;
    v.widgets.hovered.bg_stroke = Stroke::new(1.5, pal.accent);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, pal.text);
    v.widgets.hovered.corner_radius = egui::CornerRadius::same(tok.r_md as u8);

    v.widgets.active.bg_fill = pal.bg_overlay0;
    v.widgets.active.weak_bg_fill = pal.bg_overlay0;
    v.widgets.active.bg_stroke = Stroke::new(1.5, pal.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.5, pal.accent);
    v.widgets.active.corner_radius = egui::CornerRadius::same(tok.r_md as u8);

    v.widgets.open.bg_fill = pal.bg_surface0;
    v.widgets.open.fg_stroke = Stroke::new(1.0, pal.accent);
    v.widgets.open.corner_radius = egui::CornerRadius::same(tok.r_md as u8);

    v.selection.bg_fill =
        Color32::from_rgba_unmultiplied(pal.accent.r(), pal.accent.g(), pal.accent.b(), 45);
    v.selection.stroke = Stroke::new(1.0, pal.accent);
    v.override_text_color = Some(pal.text);

    ctx.set_visuals(v);

    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = Vec2::new(tok.sp8, 5.0);
    style.spacing.button_padding = Vec2::new(tok.sp10, 5.0);
    style.spacing.window_margin = Margin::same(tok.sp16 as i8);
    style.spacing.indent = 8.0;
    ctx.set_global_style(style);
}

fn apply_font_size(ctx: &egui::Context, size: f32) {
    let mut style = (*ctx.global_style()).clone();
    style.text_styles = [
        (
            egui::TextStyle::Small,
            FontId::new(size * 0.85, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            FontId::new(size, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Button,
            FontId::new(size, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Heading,
            FontId::new(size * 1.35, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            FontId::new(size, egui::FontFamily::Monospace),
        ),
    ]
    .into();
    ctx.set_global_style(style);
}

// ── Codicons glyph constants (VS Code icon font, MIT) ────────────────────────
#[allow(dead_code)]
mod icons {
    pub const SETTINGS: &str = "\u{EAF8}"; // gear
    pub const HISTORY: &str = "\u{EA82}"; // history / clock
    pub const REPLACE: &str = "\u{EBCB}"; // arrow-swap
    pub const COPY: &str = "\u{EBCC}"; // copy / duplicate
    pub const FOLDER: &str = "\u{EA83}"; // folder
    pub const EXPORT: &str = "\u{EBAC}"; // export
    pub const ADD: &str = "\u{EA60}"; // add / plus
    pub const CLOSE: &str = "\u{EA76}"; // close / ×
    pub const PLAY: &str = "\u{EB37}"; // play / run
    pub const TRASH: &str = "\u{EA81}"; // trash / delete
    pub const CHEVRON_RIGHT: &str = "\u{EAB6}"; // disclosure (collapsed)
    pub const CHEVRON_DOWN: &str = "\u{EAB4}"; // disclosure (expanded)
    pub const LIST_FLAT: &str = "\u{EB84}"; // flat list view
    pub const LIST_TREE: &str = "\u{EB86}"; // tree view
    pub const WRAP: &str = "\u{EB80}"; // word-wrap on
    pub const WRAP_OFF: &str = "\u{EB25}"; // word-wrap off (no-newline)
    pub const CHEVRON_UP: &str = "\u{EAB7}";
    pub const FOLD_UP: &str = "\u{EAF4}"; // chevron up to a bar
    pub const FOLD_DOWN: &str = "\u{EAF3}"; // chevron down to a bar
    pub const EDIT: &str = "\u{EA73}"; // pencil
    pub const CHECK: &str = "\u{EAB2}"; // checkmark
    pub const GRABBER: &str = "\u{EB02}"; // grabber / drag handle
    pub const WARNING: &str = "\u{EA6C}"; // warning / alert triangle
}

fn icon_rt(glyph: &str, size: f32, color: Color32) -> RichText {
    RichText::new(glyph)
        .font(egui::FontId::new(
            size,
            egui::FontFamily::Name("Icons".into()),
        ))
        .color(color)
}

fn setup_fonts(ctx: &egui::Context, custom_font_path: &str) {
    let mut fonts = egui::FontDefinitions::default();

    // Embedded icon font (VS Code Codicons, MIT license)
    fonts.font_data.insert(
        "Icons".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/codicon.ttf"
        ))),
    );
    fonts.families.insert(
        egui::FontFamily::Name("Icons".into()),
        vec!["Icons".to_owned()],
    );

    // Symbol/icon fonts: loaded before CJK so ⚙ ☰ ↔ etc. are available
    let symbol_paths: &[&str] = &[
        "/System/Library/Fonts/Apple Symbols.ttf",       // macOS
        "C:\\Windows\\Fonts\\seguisym.ttf",              // Windows – Segoe UI Symbol
        "/usr/share/fonts/truetype/symbola/Symbola.ttf", // Linux (optional)
    ];
    for path in symbol_paths {
        if let Ok(data) = std::fs::read(path) {
            fonts.font_data.insert(
                "Symbols".to_owned(),
                Arc::new(egui::FontData::from_owned(data)),
            );
            // Insert after NotoSans (index 1) so NotoSans stays primary for Latin text
            let prop = fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default();
            if !prop.is_empty() {
                prop.insert(1, "Symbols".to_owned());
            } else {
                prop.push("Symbols".to_owned());
            }
            break;
        }
    }

    // CJK fallback for Japanese/Chinese text.
    // CJK fonts typically have a larger ascent than egui's built-in Latin fonts,
    // causing glyphs to render too high. y_offset pushes them back down to align
    // with the Latin baseline. Positive = downward shift.
    let cjk_tweak = egui::FontTweak {
        y_offset: 2.0,
        hinting_override: Some(false), // disable per-glyph pixel snapping to reduce jaggedness
        ..Default::default()
    };
    let mut cjk_loaded = false;
    if !custom_font_path.trim().is_empty() {
        if let Ok(data) = std::fs::read(custom_font_path.trim()) {
            fonts.font_data.insert(
                "CJKFallback".to_owned(),
                Arc::new(egui::FontData::from_owned(data).tweak(cjk_tweak.clone())),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("CJKFallback".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("CJKFallback".to_owned());
            cjk_loaded = true;
        }
    }

    if !cjk_loaded {
        let cjk_paths: &[&str] = &[
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/Library/Fonts/Arial Unicode MS.ttf",
            "C:\\Windows\\Fonts\\YuGothR.ttc",
            "C:\\Windows\\Fonts\\meiryo.ttc",
            "C:\\Windows\\Fonts\\msgothic.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansJP-Regular.ttf",
        ];
        for path in cjk_paths {
            if let Ok(data) = std::fs::read(path) {
                fonts.font_data.insert(
                    "CJKFallback".to_owned(),
                    Arc::new(egui::FontData::from_owned(data).tweak(cjk_tweak)),
                );
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push("CJKFallback".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("CJKFallback".to_owned());
                break;
            }
        }
    }

    ctx.set_fonts(fonts);
}

// ── Path helpers ──────────────────────────────────────────────────────────────
/// Truncate a relative path to roughly fit `max_w` pixels given `char_w` px/char.
/// Shows ".../<last-segment>" or ".../<parent>/<last-segment>" as needed.
fn truncate_path(rel: &str, max_w: f32, char_w: f32) -> String {
    let char_budget = (max_w / char_w).floor() as usize;
    if rel.chars().count() <= char_budget {
        return rel.to_string();
    }
    // Try to show enough tail segments to fit
    let sep = if rel.contains('/') { '/' } else { '\\' };
    let segments: Vec<&str> = rel.split(sep).collect();
    let mut tail = String::new();
    for seg in segments.iter().rev() {
        let candidate = if tail.is_empty() {
            seg.to_string()
        } else {
            format!("{}{}{}", seg, sep, tail)
        };
        if candidate.chars().count() + 4 <= char_budget {
            tail = candidate;
        } else {
            break;
        }
    }
    if tail.is_empty() || tail == rel {
        // Can't shorten meaningfully or no change — fall back to last segment
        let last = rel.split(sep).next_back().unwrap_or(rel);
        format!("...{}{}", sep, last)
    } else {
        format!("...{}{}", sep, tail)
    }
}

// ── Ghost button helper ───────────────────────────────────────────────────────
/// Renders a clickable label that shows a subtle hover background and pointer
/// cursor — a "ghost" button style that doesn't look like a box at rest.
fn ghost_button(ui: &mut Ui, pal: Pal, text: RichText) -> egui::Response {
    let widget_text = egui::WidgetText::from(text);
    let galley = widget_text.into_galley(ui, None, f32::INFINITY, egui::TextStyle::Body);
    let size = galley.size();
    let (rect, resp) = ui.allocate_exact_size(size + Vec2::new(4.0, 0.0), egui::Sense::click());

    if resp.hovered() {
        ui.painter().rect_filled(
            rect.expand(2.0),
            egui::CornerRadius::same(3),
            pal.bg_surface0,
        );
    }

    ui.painter()
        .galley(rect.min + Vec2::new(2.0, 0.0), galley, pal.text);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn preset_icon_btn(
    ui: &mut Ui,
    pal: Pal,
    icon: &'static str,
    size: f32,
    color: Color32,
    tooltip: &str,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(20.0, 22.0), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(
            rect.shrink(1.0),
            egui::CornerRadius::same(3),
            pal.bg_surface0,
        );
    }
    ui.put(
        rect,
        egui::Label::new(icon_rt(icon, size, color)).selectable(false),
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip)
}

fn ghost_icon_button(ui: &mut Ui, pal: Pal, icon: &'static str, is_active: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(22.0, 22.0), egui::Sense::click());
    let is_hovered = resp.hovered();

    if is_hovered {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(3), pal.bg_surface0);
    }

    let color = if is_active {
        pal.green
    } else if is_hovered {
        pal.accent
    } else {
        pal.subtext
    };

    let icon_str = if is_active { icons::CHECK } else { icon };

    ui.put(
        rect,
        egui::Label::new(icon_rt(icon_str, 15.0, color)).selectable(false),
    );

    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

// ── Misc helpers ──────────────────────────────────────────────────────────────
fn toggle_selection(set: &mut BTreeSet<PathBuf>, path: &Path) {
    if set.contains(path) {
        set.remove(path);
    } else {
        set.insert(path.to_path_buf());
    }
}

fn settings_row(ui: &mut Ui, pal: Pal, label: &str, content: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let row_h = ui.spacing().interact_size.y;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(152.0, row_h), egui::Sense::hover());
        ui.painter().text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.0),
            pal.subtext,
        );
        content(ui);
    });
    ui.add_space(4.0);
}

/// Renders the filter+flag cluster (Inc / Exc / Regex / Case / Word / Ctx / Depth).
/// Returns (inc_resp, inc_filtered, exc_resp, exc_filtered) so the caller can render
/// suggestion popups outside the horizontal block.
#[allow(clippy::too_many_arguments)]
fn show_filter_flags(
    ui: &mut Ui,
    params: &mut crate::models::SearchParams,
    pal: Pal,
    inc_id: egui::Id,
    exc_id: egui::Id,
    dir_id: egui::Id,
    pat_id: egui::Id,
    recent_includes: &[String],
    recent_excludes: &[String],
    inc_popup_id: egui::Id,
    exc_popup_id: egui::Id,
    inc_suggest_idx: &mut Option<usize>,
    exc_suggest_idx: &mut Option<usize>,
) -> (
    Option<egui::Response>,
    Vec<String>,
    Option<egui::Response>,
    Vec<String>,
) {
    let row_h = ui.spacing().interact_size.y;
    let hint = |text: &str| RichText::new(text).color(pal.placeholder).italics();

    ui.label(RichText::new("Include:").size(12.0))
        .on_hover_text("Include files matching this glob (e.g. *.rs)");
    let inc_resp = ui.add(
        egui::TextEdit::singleline(&mut params.file_glob)
            .id(inc_id)
            .hint_text(hint("*.rs"))
            .desired_width(300.0),
    );

    let iq = params.file_glob.to_lowercase();
    let inc_filtered: Vec<String> = recent_includes
        .iter()
        .filter(|v| iq.is_empty() || v.to_lowercase().contains(&iq))
        .take(8)
        .cloned()
        .collect();

    if inc_resp.changed() {
        if !inc_filtered.is_empty() && inc_resp.has_focus() {
            egui::Popup::open_id(ui.ctx(), inc_popup_id);
        } else if inc_filtered.is_empty() {
            egui::Popup::close_id(ui.ctx(), inc_popup_id);
        }
        *inc_suggest_idx = None;
    }
    if (inc_resp.gained_focus() || inc_resp.clicked()) && !inc_filtered.is_empty() {
        egui::Popup::open_id(ui.ctx(), inc_popup_id);
        *inc_suggest_idx = None;
    }

    let inc_popup_open = egui::Popup::is_id_open(ui.ctx(), inc_popup_id);
    let inc_n = inc_filtered.len();

    if inc_resp.has_focus() {
        if inc_popup_open {
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
                *inc_suggest_idx =
                    Some(inc_suggest_idx.map_or(0, |i| (i + 1).min(inc_n.saturating_sub(1))));
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                *inc_suggest_idx = Some(inc_suggest_idx.map_or(0, |i| i.saturating_sub(1)));
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                egui::Popup::close_id(ui.ctx(), inc_popup_id);
                *inc_suggest_idx = None;
            }
        } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
            && !inc_filtered.is_empty()
        {
            egui::Popup::open_id(ui.ctx(), inc_popup_id);
            *inc_suggest_idx = Some(0);
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
            ui.ctx().memory_mut(|m| m.request_focus(exc_id));
            egui::Popup::close_id(ui.ctx(), inc_popup_id);
            *inc_suggest_idx = None;
        } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab)) {
            ui.ctx().memory_mut(|m| m.request_focus(pat_id));
            egui::Popup::close_id(ui.ctx(), inc_popup_id);
            *inc_suggest_idx = None;
        }
    }

    ui.label(RichText::new("Exclude:").size(12.0))
        .on_hover_text("Exclude paths (comma-separated globs, e.g. node_modules,*.min.js)");
    let exc_resp = ui.add(
        egui::TextEdit::singleline(&mut params.exclude_glob)
            .id(exc_id)
            .hint_text(hint("node_modules,*.min.js"))
            .desired_width(300.0),
    );

    let eq = params.exclude_glob.to_lowercase();
    let exc_filtered: Vec<String> = recent_excludes
        .iter()
        .filter(|v| eq.is_empty() || v.to_lowercase().contains(&eq))
        .take(8)
        .cloned()
        .collect();

    if exc_resp.changed() {
        if !exc_filtered.is_empty() && exc_resp.has_focus() {
            egui::Popup::open_id(ui.ctx(), exc_popup_id);
        } else if exc_filtered.is_empty() {
            egui::Popup::close_id(ui.ctx(), exc_popup_id);
        }
        *exc_suggest_idx = None;
    }
    if (exc_resp.gained_focus() || exc_resp.clicked()) && !exc_filtered.is_empty() {
        egui::Popup::open_id(ui.ctx(), exc_popup_id);
        *exc_suggest_idx = None;
    }

    let exc_popup_open = egui::Popup::is_id_open(ui.ctx(), exc_popup_id);
    let exc_n = exc_filtered.len();

    if exc_resp.has_focus() {
        if exc_popup_open {
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
                *exc_suggest_idx =
                    Some(exc_suggest_idx.map_or(0, |i| (i + 1).min(exc_n.saturating_sub(1))));
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                *exc_suggest_idx = Some(exc_suggest_idx.map_or(0, |i| i.saturating_sub(1)));
            }
            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                egui::Popup::close_id(ui.ctx(), exc_popup_id);
                *exc_suggest_idx = None;
            }
        } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
            && !exc_filtered.is_empty()
        {
            egui::Popup::open_id(ui.ctx(), exc_popup_id);
            *exc_suggest_idx = Some(0);
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
            ui.ctx().memory_mut(|m| m.request_focus(dir_id));
            egui::Popup::close_id(ui.ctx(), exc_popup_id);
            *exc_suggest_idx = None;
        } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab)) {
            ui.ctx().memory_mut(|m| m.request_focus(inc_id));
            egui::Popup::close_id(ui.ctx(), exc_popup_id);
            *exc_suggest_idx = None;
        }
    }

    ui.separator();

    ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
    ui.checkbox(&mut params.is_regex, "Regex")
        .on_hover_text("Regular expression mode");
    ui.checkbox(&mut params.case_sensitive, "Case")
        .on_hover_text("Case sensitive");
    ui.checkbox(&mut params.word_boundary, "Word")
        .on_hover_text("Whole word only (\\b...\\b)");
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);

    // Vertical alignment fix (BL-72):
    // A DragValue renders as a Button whose intrinsic height is
    //   galley_height + 2*button_padding.y  (≈17 + 2 = 19px)
    // which exceeds the horizontal row height (interact_size.y = 18px). egui then
    // applies its "expand downward so we don't overlap the row above" hack
    // (Layout::next_frame_ignore_wrap), pushing the box ~1px below the label baseline.
    // Zeroing button_padding.y caps the intrinsic height at interact_size.y (18px),
    // so the box centers in the row exactly like the labels and checkboxes.
    let saved_padding = ui.spacing().button_padding;
    ui.spacing_mut().button_padding.y = 0.0;

    ui.label(RichText::new("Context:").size(12.0))
        .on_hover_text("Context lines before/after each match");
    ui.add_sized(
        [36.0, row_h],
        egui::DragValue::new(&mut params.context_lines)
            .range(0..=10)
            .speed(0.1),
    );

    ui.add_space(4.0);

    let mut depth_val = params.max_depth.unwrap_or(0);
    ui.label(RichText::new("Depth:").size(12.0))
        .on_hover_text("Maximum search depth (0 = unlimited)");
    let depth_resp = ui.add_sized(
        [36.0, row_h],
        egui::DragValue::new(&mut depth_val)
            .range(0..=100)
            .speed(0.1)
            .custom_formatter(|val, _| {
                if val == 0.0 {
                    "All".to_string()
                } else {
                    format!("{:.0}", val)
                }
            }),
    );
    if depth_resp.changed() {
        params.max_depth = if depth_val == 0 {
            None
        } else {
            Some(depth_val)
        };
    }

    ui.spacing_mut().button_padding = saved_padding;

    (Some(inc_resp), inc_filtered, Some(exc_resp), exc_filtered)
}

/// Renders a Type-preset chip with correct paint order (background drawn before text).
/// Using `painter.rect_filled()` after `ui.add(label)` would draw the rect ON TOP of
/// the text, hiding it. This helper allocates the rect first, paints bg, then paints text.
fn preset_chip(
    ui: &mut Ui,
    pal: Pal,
    label: &str,
    active: bool,
    tooltip: &str,
    on_click: impl FnOnce(),
) {
    let font_id = egui::FontId::new(11.0, egui::FontFamily::Proportional);
    let text_color = if active { pal.accent } else { pal.subtext };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_string(), font_id, text_color);
    let padding = Vec2::new(4.0, 2.0);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + padding * 2.0, egui::Sense::click());
    let resp = resp.on_hover_text(tooltip);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if active || resp.hovered() {
            let bg = if active {
                pal.bg_surface1
            } else {
                pal.bg_surface0
            };
            painter.rect_filled(rect, 3.0, bg);
        }
        painter.galley(rect.min + padding, galley, text_color);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        on_click();
    }
}

fn toolbar_frame(pal: Pal) -> egui::Frame {
    egui::Frame::NONE.fill(pal.bg_mantle).inner_margin(Margin {
        left: 10,
        right: 10,
        top: 4,
        bottom: 2,
    })
}

fn open_in_editor(path: &Path, line: Option<usize>, editor_cmd: &str) {
    if editor_cmd.trim().is_empty() {
        return;
    }
    let mut parts = editor_cmd.split_whitespace();
    let Some(prog) = parts.next() else { return };
    let mut cmd = std::process::Command::new(prog);
    for arg in parts {
        cmd.arg(arg);
    }
    if let Some(ln) = line {
        cmd.arg(format!("{}:{}", path.display(), ln));
    } else {
        cmd.arg(path);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.spawn();
}

fn short_dir(dir: &str) -> String {
    let p = Path::new(dir);
    let parts: Vec<_> = p.components().collect();
    if parts.len() <= 3 {
        return dir.to_string();
    }
    format!(
        ".../{}",
        parts[parts.len() - 2..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    )
}

fn format_ts(ts: &str) -> String {
    ts.get(..16)
        .map(|s| s.replace('T', "  "))
        .unwrap_or_else(|| ts.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // BL-49: char-boundary helpers
    #[test]
    fn test_floor_char_boundary_ascii() {
        assert_eq!(floor_char_boundary("hello", 3), 3);
    }

    #[test]
    fn test_floor_char_boundary_multibyte() {
        let s = "日本語"; // each char is 3 bytes
                          // byte 4 is inside the second char; floor should give 3
        assert_eq!(floor_char_boundary(s, 4), 3);
    }

    #[test]
    fn test_ceil_char_boundary_multibyte() {
        let s = "日本語";
        // byte 4 is inside the second char; ceil should give 6
        assert_eq!(ceil_char_boundary(s, 4), 6);
    }

    // BL-17 regression: chars().take(N) never panics on multibyte strings
    #[test]
    fn test_multibyte_truncate_no_panic() {
        let long_jp = "あいうえおかきくけこさしすせそたちつてと"; // 20 chars, 60 bytes
        let truncated: String = long_jp.chars().take(30).collect();
        assert_eq!(truncated, long_jp); // 20 < 30, so full string
        let truncated30: String = long_jp.chars().take(10).collect();
        assert_eq!(truncated30.chars().count(), 10);
        assert!(long_jp.is_char_boundary(truncated30.len()));
    }

    // BL-49: short_dir
    #[test]
    fn test_short_dir_short_path_unchanged() {
        assert_eq!(short_dir("/a/b"), "/a/b");
    }

    #[test]
    fn test_short_dir_long_path_abbreviated() {
        let result = short_dir("/very/long/path/to/project");
        assert!(
            result.starts_with("..."),
            "expected abbreviation, got: {}",
            result
        );
        assert!(
            result.ends_with("to/project"),
            "expected tail, got: {}",
            result
        );
    }

    // BL-49: format_ts
    #[test]
    fn test_format_ts_replaces_t() {
        assert_eq!(format_ts("2026-01-15T12:34:56"), "2026-01-15  12:34");
    }

    #[test]
    fn test_format_ts_short_string_unchanged() {
        assert_eq!(format_ts("short"), "short");
    }

    // ── Intra-line diff helpers (BL-69) ───────────────────────────────────────
    // common_prefix_len / common_suffix_len drive the replace-preview word-level
    // diff. They return BYTE offsets used to slice the line; a non-char-boundary
    // result would panic at runtime on multibyte content (BL-17 class bug).

    #[test]
    fn test_common_prefix_len_ascii() {
        assert_eq!(common_prefix_len("let foo = 1", "let bar = 1"), 4); // "let "
        assert_eq!(common_prefix_len("abc", "abc"), 3);
        assert_eq!(common_prefix_len("xyz", "abc"), 0);
    }

    #[test]
    fn test_common_suffix_len_ascii() {
        // "let foo = 1" vs "let bar = 1": common suffix " = 1" = 4 bytes
        let pfx = common_prefix_len("let foo = 1", "let bar = 1");
        assert_eq!(common_suffix_len("let foo = 1", "let bar = 1", pfx), 4);
    }

    #[test]
    fn test_diff_slicing_multibyte_no_panic() {
        // Changed region foo→bar surrounded by multibyte text. Mirrors the
        // slicing done in show_replace_preview_window so we lock in that the
        // byte offsets always land on char boundaries.
        let orig = "変数foo設定する";
        let new = "変数bar設定する";
        let pfx = common_prefix_len(orig, new);
        let sfx = common_suffix_len(orig, new, pfx);
        let ch_start = pfx;
        let ch_end = new.len().saturating_sub(sfx);
        assert!(
            new.is_char_boundary(ch_start),
            "ch_start not on char boundary"
        );
        assert!(new.is_char_boundary(ch_end), "ch_end not on char boundary");
        assert!(ch_start < ch_end);
        // The highlighted (changed) slice must not panic and should be the ASCII edit.
        assert_eq!(&new[ch_start..ch_end], "bar");
    }

    #[test]
    fn test_diff_identical_lines_no_change() {
        let s = "unchanged 日本語 line";
        let pfx = common_prefix_len(s, s);
        let sfx = common_suffix_len(s, s, pfx);
        // Whole string is common prefix; suffix beyond prefix is 0.
        assert_eq!(pfx, s.len());
        assert_eq!(sfx, 0);
    }

    // ── truncate_path (BL-37) ─────────────────────────────────────────────────

    #[test]
    fn test_truncate_path_short_unchanged() {
        // Wide budget: 200px / 7.5px ≈ 26 chars, path is shorter.
        assert_eq!(truncate_path("src/app.rs", 200.0, 7.5), "src/app.rs");
    }

    #[test]
    fn test_truncate_path_long_abbreviated() {
        let out = truncate_path("very/deep/nested/path/to/module/file.rs", 100.0, 7.5);
        assert!(
            out.starts_with("..."),
            "expected ellipsis prefix, got: {out}"
        );
        assert!(
            out.ends_with("file.rs"),
            "expected basename retained, got: {out}"
        );
    }

    #[test]
    fn test_truncate_path_multibyte_no_panic() {
        // Narrow budget forces truncation of a multibyte path — must not panic.
        let out = truncate_path("プロジェクト/ソース/メイン.rs", 60.0, 7.5);
        assert!(
            out.ends_with("メイン.rs"),
            "expected multibyte basename, got: {out}"
        );
    }

    #[test]
    fn test_format_match_line_placeholders() {
        use crate::models::MatchRange;
        let pattern = "fn";
        let search_root = "/app";
        let file_path = Path::new("/app/src/main.rs");
        let line_number = 42;
        let line_content = "pub fn main() {}";
        let ranges = vec![MatchRange { start: 4, end: 6 }];

        let format_str = "%f: %n -> %l [%m]%N%Ttabbed";
        let formatted = format_match_line(
            pattern,
            search_root,
            file_path,
            line_number,
            line_content,
            &ranges,
            format_str,
        );

        let expected = "src/main.rs: 42 -> pub fn main() {} [fn]\n\ttabbed";
        assert_eq!(formatted, expected);
    }

    #[test]
    fn test_format_matches_to_string_flat_and_grouped() {
        use crate::models::{FileMatch, LineMatch, MatchRange};
        let mut config = Config::default();

        let fm1 = FileMatch {
            path: PathBuf::from("/app/src/a.rs"),
            matches: vec![LineMatch {
                line_number: 1,
                content: "fn foo()".to_string(),
                ranges: vec![MatchRange { start: 0, end: 2 }],
                is_match: true,
            }],
        };
        let fm2 = FileMatch {
            path: PathBuf::from("/app/src/b.rs"),
            matches: vec![LineMatch {
                line_number: 10,
                content: "fn bar()".to_string(),
                ranges: vec![MatchRange { start: 0, end: 2 }],
                is_match: true,
            }],
        };

        let params = crate::models::SearchParams {
            pattern: "fn".to_string(),
            directory: "/app".to_string(),
            ..crate::models::SearchParams::default()
        };

        // Flat mode test
        config.export_output_mode = crate::config::ExportOutputMode::Flat;
        config.export_line_format = "%f:%n:%l".to_string();
        let formatted_flat = format_matches_to_string_impl(&config, &[&fm1, &fm2], &params);
        let expected_flat = if cfg!(target_os = "windows") {
            "src/a.rs:1:fn foo()\r\nsrc/b.rs:10:fn bar()"
        } else {
            "src/a.rs:1:fn foo()\nsrc/b.rs:10:fn bar()"
        };
        assert_eq!(formatted_flat, expected_flat);

        // Grouped mode test
        config.export_output_mode = crate::config::ExportOutputMode::Grouped;
        config.export_file_header_format = "### %f".to_string();
        config.export_line_format = "  - %n: %l".to_string();
        let formatted_grouped = format_matches_to_string_impl(&config, &[&fm1, &fm2], &params);
        let expected_grouped = if cfg!(target_os = "windows") {
            "### src/a.rs\r\n  - 1: fn foo()\r\n\r\n### src/b.rs\r\n  - 10: fn bar()"
        } else {
            "### src/a.rs\n  - 1: fn foo()\n\n### src/b.rs\n  - 10: fn bar()"
        };
        assert_eq!(formatted_grouped, expected_grouped);

        // Omit single file test
        config.export_omit_single_file_name = true;
        let formatted_omit = format_matches_to_string_impl(&config, &[&fm1], &params);
        let expected_omit = "  - 1: fn foo()";
        assert_eq!(formatted_omit, expected_omit);
    }
}
