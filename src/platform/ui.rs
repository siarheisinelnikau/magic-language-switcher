use super::hotkeys::{CAPTURE_MESSAGE, CaptureHook, Registry};
use super::{
    about,
    correction::{Action, Engine},
    keyboard,
    native::{Target, wide},
    settings, updater,
};
use crate::{
    layout::{Catalog, Selection},
    shortcuts::{Shortcut, Shortcuts},
};
use std::{
    cell::RefCell,
    mem::size_of,
    ptr::{null, null_mut},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};
use windows_sys::Win32::{
    Foundation::*,
    Globalization::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::*, SystemServices::*, Threading::*},
    UI::{Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
};

pub(super) const WINDOW_CLASS: &str = "MagicLanguageSwitcher.Main";
pub(super) const QUIT_ID: usize = 203;
const TRAY_MESSAGE: u32 = WM_APP + 1;
const OPEN_MESSAGE: u32 = WM_APP + 2;
const PAUSE_ID: usize = 101;
const STARTUP_ID: usize = 103;
const INPUT_ID: usize = 104;
const OUTPUT_ID: usize = 105;
const ABOUT_ID: usize = 106;
const FROM_ID: usize = 109;
const TO_ID: usize = 110;
const SHORTCUT_FIRST_ID: usize = 120;
const OPEN_ID: usize = 201;
const TRAY_PAUSE_ID: usize = 202;
const UPDATE_ID: usize = 206;
const BG: u32 = rgb(246, 248, 252);
const INK: u32 = rgb(29, 43, 66);
const MUTED: u32 = rgb(99, 112, 131);
const BLUE: u32 = rgb(39, 87, 216);
const PALE: u32 = rgb(232, 237, 245);
const GREEN: u32 = rgb(229, 244, 239);
const WHITE: u32 = rgb(255, 255, 255);
const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}

struct Widget {
    handle: HWND,
    rect: [i32; 4],
    font: usize,
    foreground: u32,
    background: u32,
}
struct Request {
    action: Action,
    target: Target,
    selection: Selection,
    catalog: Arc<Catalog>,
}

#[derive(Clone, Copy)]
struct Capture {
    index: usize,
    pending: Option<Shortcut>,
    release_key: u16,
    released: bool,
    held_modifiers: u16,
}

enum UpdateResult {
    Checked(Result<Option<crate::updates::Update>, String>),
    Installed(Result<(), String>),
}
struct UpdateJob {
    results: mpsc::Receiver<UpdateResult>,
    cancel: Arc<AtomicBool>,
    worker: thread::JoinHandle<()>,
}

struct App {
    window: HWND,
    tray_icon: HICON,
    icon: HICON,
    instance: HINSTANCE,
    scale: f32,
    widgets: Vec<Widget>,
    fonts: Vec<HFONT>,
    brushes: Vec<(u32, HBRUSH)>,
    status: HWND,
    subtitle: HWND,
    input: HWND,
    from: HWND,
    to: HWND,
    selection: Selection,
    catalog: Arc<Catalog>,
    output: HWND,
    enabled: bool,
    busy: bool,
    exit_requested: bool,
    conflicts: Vec<String>,
    registrations: Option<Registry>,
    shortcuts: Shortcuts,
    capture: Option<Capture>,
    capture_hook: Option<CaptureHook>,
    tray_added: bool,
    taskbar_message: u32,
    last_language: String,
    work: Option<mpsc::Sender<Request>>,
    results: Option<mpsc::Receiver<Result<String, String>>>,
    worker: Option<thread::JoinHandle<()>>,
    update: Option<UpdateJob>,
}

impl App {
    fn new(instance: HINSTANCE, icon: HICON, scale: f32) -> Self {
        let preferences = settings::load();
        Self {
            window: null_mut(),
            tray_icon: null_mut(),
            instance,
            icon,
            scale,
            widgets: Vec::new(),
            fonts: Vec::new(),
            brushes: Vec::new(),
            status: null_mut(),
            subtitle: null_mut(),
            input: null_mut(),
            from: null_mut(),
            to: null_mut(),
            selection: preferences.selection,
            catalog: Arc::new(keyboard::installed()),
            output: null_mut(),
            enabled: true,
            busy: false,
            exit_requested: false,
            conflicts: Vec::new(),
            registrations: None,
            shortcuts: preferences.shortcuts,
            capture: None,
            capture_hook: None,
            tray_added: false,
            taskbar_message: 0,
            last_language: String::new(),
            work: None,
            results: None,
            worker: None,
            update: None,
        }
    }
    fn s(&self, value: i32) -> i32 {
        (value as f32 * self.scale).round() as i32
    }
    fn brush(&mut self, color: u32) -> HBRUSH {
        if let Some((_, brush)) = self.brushes.iter().find(|(value, _)| *value == color) {
            return *brush;
        }
        // SAFETY: each created brush is retained until all windows are destroyed.
        let brush = unsafe { CreateSolidBrush(color) };
        self.brushes.push((color, brush));
        brush
    }
    fn create_fonts(&mut self) {
        self.fonts = [
            (14, 400),
            (12, 600),
            (22, 700),
            (15, 700),
            (12, 400),
            (24, 700),
        ]
        .into_iter()
        .map(|(height, weight)| {
            // SAFETY: the UTF-16 font family is valid for the duration of this call.
            unsafe {
                CreateFontW(
                    -self.s(height),
                    0,
                    0,
                    0,
                    weight,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as u32,
                    0,
                    0,
                    CLEARTYPE_QUALITY as u32,
                    0,
                    wide("Segoe UI").as_ptr(),
                )
            }
        })
        .collect();
    }
    #[allow(clippy::too_many_arguments)]
    fn widget(
        &mut self,
        class: &str,
        text: &str,
        rect: [i32; 4],
        id: usize,
        style: u32,
        font: usize,
        foreground: u32,
        background: u32,
    ) -> HWND {
        // SAFETY: the parent window and instance are live, and Windows copies all string data.
        let handle = unsafe {
            let handle = CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(text).as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                self.s(rect[0]),
                self.s(rect[1]),
                self.s(rect[2]),
                self.s(rect[3]),
                self.window,
                id as HMENU,
                self.instance,
                null(),
            );
            SendMessageW(handle, WM_SETFONT, self.fonts[font] as usize, 1);
            handle
        };
        self.widgets.push(Widget {
            handle,
            rect,
            font,
            foreground,
            background,
        });
        handle
    }
    fn label(
        &mut self,
        text: &str,
        rect: [i32; 4],
        font: usize,
        foreground: u32,
        background: u32,
    ) -> HWND {
        self.widget(
            "STATIC",
            text,
            rect,
            0,
            SS_LEFT | SS_NOPREFIX,
            font,
            foreground,
            background,
        )
    }
    fn initialize(&mut self) -> Result<(), String> {
        self.create_fonts();
        self.label("Magic Language Switcher", [24, 22, 416, 30], 2, INK, BG);
        self.label(
            "Selection first; otherwise the last word.",
            [24, 57, 416, 20],
            4,
            MUTED,
            BG,
        );
        self.status = self.widget(
            "STATIC",
            "●  Ready",
            [38, 100, 260, 24],
            108,
            SS_LEFT | SS_NOPREFIX,
            3,
            rgb(27, 105, 83),
            GREEN,
        );
        self.widget(
            "BUTTON",
            "Pause",
            [344, 94, 84, 30],
            PAUSE_ID,
            WS_TABSTOP | BS_PUSHBUTTON as u32,
            0,
            INK,
            GREEN,
        );
        self.label("From", [24, 153, 198, 18], 4, MUTED, BG);
        self.from = self.widget(
            "COMBOBOX",
            "",
            [24, 175, 198, 220],
            FROM_ID,
            WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
            0,
            INK,
            WHITE,
        );
        self.label("To", [242, 153, 198, 18], 4, MUTED, BG);
        self.to = self.widget(
            "COMBOBOX",
            "",
            [242, 175, 198, 220],
            TO_ID,
            WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
            0,
            INK,
            WHITE,
        );
        self.populate_layouts();
        for (index, (y, title)) in [(222, "Correct text"), (261, "Undo last fix")]
            .into_iter()
            .enumerate()
        {
            self.label(title, [24, y + 5, 204, 25], 0, INK, BG);
            self.widget(
                "BUTTON",
                &self.shortcuts.0[index].to_string(),
                [242, y, 198, 30],
                SHORTCUT_FIRST_ID + index,
                WS_TABSTOP | BS_PUSHBUTTON as u32,
                1,
                INK,
                PALE,
            );
        }
        self.label("Try it here", [36, 320, 392, 22], 1, INK, WHITE);
        self.input = self.widget(
            "EDIT",
            "",
            [36, 352, 170, 27],
            INPUT_ID,
            WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32,
            0,
            INK,
            WHITE,
        );
        self.output = self.widget(
            "EDIT",
            "",
            [258, 352, 170, 27],
            OUTPUT_ID,
            WS_TABSTOP | WS_BORDER | (ES_AUTOHSCROLL | ES_READONLY) as u32,
            0,
            INK,
            BG,
        );
        self.label("→", [222, 351, 28, 28], 3, BLUE, WHITE);
        self.subtitle = self.widget(
            "STATIC",
            "Click a shortcut to change it.",
            [24, 407, 416, 20],
            107,
            SS_LEFT | SS_NOPREFIX | SS_ENDELLIPSIS,
            4,
            MUTED,
            BG,
        );
        let startup = self.widget(
            "BUTTON",
            "Start with Windows",
            [24, 441, 275, 26],
            STARTUP_ID,
            WS_TABSTOP | BS_AUTOCHECKBOX as u32,
            0,
            INK,
            BG,
        );
        self.widget(
            "BUTTON",
            "About",
            [356, 439, 84, 28],
            ABOUT_ID,
            WS_TABSTOP | BS_PUSHBUTTON as u32,
            4,
            INK,
            BG,
        );
        // SAFETY: the checkbox and root were created on this thread; hotkeys belong to root.
        unsafe {
            SendMessageW(
                startup,
                BM_SETCHECK,
                usize::from(settings::startup_enabled()),
                0,
            );
            self.taskbar_message = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
            SetTimer(self.window, 1, 150, None);
        }
        let mut registrations = Registry::new(self.window);
        registrations.resume(self.shortcuts);
        self.registrations = Some(registrations);
        self.refresh_status();
        let (sender, work) = mpsc::channel::<Request>();
        let (result, results) = mpsc::channel();
        let owner = self.window as usize;
        self.worker = Some(
            thread::Builder::new()
                .name("text-correction".into())
                .spawn(move || {
                    let mut engine = Engine::default();
                    for request in work {
                        if result
                            .send(engine.execute(
                                request.action,
                                request.target,
                                owner,
                                &request.catalog,
                                request.selection,
                            ))
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .map_err(|e| format!("Could not start the correction worker: {e}"))?,
        );
        self.work = Some(sender);
        self.results = Some(results);
        self.update_preview();
        self.refresh_tray_language();
        self.update_tray(NIM_ADD);
        if !self.tray_added {
            return Err("Windows could not create the system tray icon.".into());
        }
        if !self.conflicts.is_empty() {
            self.set_text(self.status, "●  Shortcut conflict");
            self.set_text(
                self.subtitle,
                &format!("Already in use: {}", self.conflicts.join(", ")),
            );
            self.notify(&format!("Already in use: {}. Click the unavailable shortcut in settings to choose another combination.", self.conflicts.join(", ")));
        }
        Ok(())
    }
    fn set_text(&self, handle: HWND, text: &str) {
        // SAFETY: the handle refers to one of our live controls; Windows copies the text.
        unsafe {
            SetWindowTextW(handle, wide(text).as_ptr());
        }
    }
    fn update_preview(&self) {
        let input = window_text(self.input);
        let output = self
            .catalog
            .convert(&input, self.selection, keyboard::active())
            .map_or_else(|_| String::new(), |conversion| conversion.text);
        self.set_text(self.output, &output);
    }
    fn populate_layouts(&mut self) {
        if !self
            .catalog
            .layouts
            .iter()
            .any(|layout| Some(layout.id) == self.selection.from)
        {
            self.selection.from = None;
        }
        if !self
            .catalog
            .layouts
            .iter()
            .any(|layout| Some(layout.id) == self.selection.to)
        {
            self.selection.to = None;
        }
        // SAFETY: combo box messages copy strings synchronously; item data stores integer layout IDs.
        unsafe {
            for (control, automatic, selected) in [
                (self.from, "Auto (active layout)", self.selection.from),
                (self.to, "Next installed layout", self.selection.to),
            ] {
                SendMessageW(control, CB_RESETCONTENT, 0, 0);
                SendMessageW(control, CB_ADDSTRING, 0, wide(automatic).as_ptr() as isize);
                let mut choice = 0;
                for (index, layout) in self.catalog.layouts.iter().enumerate() {
                    let item = SendMessageW(
                        control,
                        CB_ADDSTRING,
                        0,
                        wide(&layout.name).as_ptr() as isize,
                    );
                    SendMessageW(control, CB_SETITEMDATA, item as usize, layout.id as isize);
                    if Some(layout.id) == selected {
                        choice = index + 1;
                    }
                }
                SendMessageW(control, CB_SETCURSEL, choice, 0);
                SendMessageW(control, CB_SETDROPPEDWIDTH, self.s(390) as usize, 0);
            }
        }
    }
    fn selected_layout(&self, control: HWND) -> Option<u32> {
        // SAFETY: these queries have no pointer arguments and target a live combo box.
        unsafe {
            let index = SendMessageW(control, CB_GETCURSEL, 0, 0);
            if index <= 0 {
                None
            } else {
                Some(SendMessageW(control, CB_GETITEMDATA, index as usize, 0) as u32)
            }
        }
    }
    fn update_tray(&mut self, operation: u32) {
        let mut data = self.tray_data();
        data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        let tip = format!(
            "Magic Language Switcher — {} — {}",
            self.last_language,
            if self.enabled { "ready" } else { "paused" }
        );
        copy_wide(&mut data.szTip, &tip);
        // SAFETY: NOTIFYICONDATAW has its native size and all buffers are inline and terminated.
        let success = unsafe { Shell_NotifyIconW(operation, &data) } != 0;
        if operation == NIM_ADD {
            self.tray_added = success;
        }
    }
    fn tray_data(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.window,
            uID: 1,
            uCallbackMessage: TRAY_MESSAGE,
            hIcon: if self.tray_icon.is_null() {
                self.icon
            } else {
                self.tray_icon
            },
            ..Default::default()
        }
    }
    fn notify(&self, message: &str) {
        // Keep isolated desktop checks from opening system toasts over their editor.
        #[cfg(feature = "desktop-tests")]
        if std::env::var_os("MAGIC_LANGUAGE_SWITCHER_SETTINGS").is_some() {
            return;
        }
        let mut data = self.tray_data();
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = NIIF_INFO;
        copy_wide(&mut data.szInfoTitle, "Magic Language Switcher");
        copy_wide(&mut data.szInfo, message);
        // SAFETY: the fixed-size notification buffers are populated with bounded UTF-16.
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }
    fn open(&mut self) {
        self.catalog = Arc::new(keyboard::installed());
        self.populate_layouts();
        self.update_preview();
        // SAFETY: root remains live for the application's message loop.
        unsafe {
            ShowWindow(self.window, SW_RESTORE);
            SetForegroundWindow(self.window);
        }
    }
    fn toggle_pause(&mut self) {
        self.enabled = !self.enabled;
        self.refresh_status();
        // SAFETY: PAUSE_ID is a child button created in initialize.
        self.set_text(
            unsafe { GetDlgItem(self.window, PAUSE_ID as i32) },
            if self.enabled { "Pause" } else { "Resume" },
        );
        self.last_language.clear();
        self.refresh_tray_language();
    }
    fn refresh_status(&mut self) {
        self.conflicts = self
            .registrations
            .as_ref()
            .map_or_else(Vec::new, |registry| registry.conflicts(self.shortcuts));
        let status = if self.capture.is_some() {
            "●  Set shortcut"
        } else if !self.enabled {
            "●  Paused"
        } else if !self.conflicts.is_empty() {
            "●  Shortcut conflict"
        } else {
            "●  Ready"
        };
        self.set_text(self.status, status);
    }
    fn shortcut_button(&self, index: usize) -> HWND {
        // SAFETY: shortcut indexes are validated before querying our two child controls.
        unsafe { GetDlgItem(self.window, (SHORTCUT_FIRST_ID + index) as i32) }
    }
    fn show_shortcuts(&self) {
        for (index, shortcut) in self.shortcuts.0.iter().enumerate() {
            self.set_text(self.shortcut_button(index), &shortcut.to_string());
        }
    }
    fn begin_capture(&mut self, index: usize) {
        if self.busy {
            self.set_text(self.subtitle, "Wait for the current correction to finish.");
            return;
        }
        if let Some(registry) = &mut self.registrations {
            registry.suspend();
        }
        match CaptureHook::start(self.window) {
            Ok(hook) => self.capture_hook = Some(hook),
            Err(error) => {
                if let Some(registry) = &mut self.registrations {
                    registry.resume(self.shortcuts);
                }
                self.refresh_status();
                self.set_text(self.subtitle, &error);
                return;
            }
        }
        self.capture = Some(Capture {
            index,
            pending: None,
            release_key: 0,
            released: false,
            held_modifiers: 0,
        });
        self.set_text(self.shortcut_button(index), "Press keys...");
        self.set_text(self.subtitle, "Press keys · Esc cancels · Backspace resets");
        self.refresh_status();
        // SAFETY: this button is owned by the UI thread and remains live during capture.
        unsafe {
            SetFocus(self.shortcut_button(index));
        }
    }
    fn cancel_capture(&mut self) {
        if self.capture.take().is_none() {
            return;
        }
        self.capture_hook.take();
        if let Some(registry) = &mut self.registrations {
            registry.resume(self.shortcuts);
        }
        self.show_shortcuts();
        self.refresh_status();
        self.set_text(self.subtitle, "Shortcut change cancelled.");
    }
    fn capture_key(&mut self, key: u16, modifiers: u16, up: bool) {
        let Some(mut capture) = self.capture else {
            return;
        };
        capture.held_modifiers = modifiers;
        if up {
            if key == capture.release_key {
                capture.released = true;
            }
            self.capture = Some(capture);
            self.finish_capture_if_released();
            return;
        }
        if capture.pending.is_some() {
            return;
        }
        if key == VK_ESCAPE || key == VK_TAB && modifiers == 0 {
            self.cancel_capture();
            return;
        }
        if matches!(
            key,
            VK_SHIFT
                | VK_CONTROL
                | VK_MENU
                | VK_LSHIFT
                | VK_RSHIFT
                | VK_LCONTROL
                | VK_RCONTROL
                | VK_LMENU
                | VK_RMENU
        ) {
            self.capture = Some(capture);
            return;
        }
        let shortcut = if key == VK_BACK && modifiers == 0 {
            Shortcuts::default().0[capture.index]
        } else {
            Shortcut { key, modifiers }
        };
        if let Err(error) = shortcut.validate() {
            self.set_text(self.subtitle, error);
            self.capture = Some(capture);
            return;
        }
        self.capture = Some(Capture {
            pending: Some(shortcut),
            release_key: key,
            released: false,
            ..capture
        });
        self.set_text(self.shortcut_button(capture.index), &shortcut.to_string());
        self.set_text(self.subtitle, "Release the keys to save this shortcut.");
    }
    fn finish_capture_if_released(&mut self) {
        let Some(capture) = self.capture else {
            return;
        };
        let Some(shortcut) = capture.pending else {
            return;
        };
        if !capture.released || capture.held_modifiers != 0 {
            return;
        }
        self.capture = None;
        self.capture_hook.take();
        let selection = self.selection;
        let result = self
            .registrations
            .as_mut()
            .ok_or_else(|| "Shortcut registration is unavailable.".to_owned())
            .and_then(|registry| {
                registry.update(self.shortcuts, capture.index, shortcut, |shortcuts| {
                    settings::save(settings::Preferences {
                        selection,
                        shortcuts,
                    })
                })
            });
        match result {
            Ok(shortcuts) => {
                self.shortcuts = shortcuts;
                self.set_text(self.subtitle, &format!("Shortcut saved: {shortcut}"));
            }
            Err(error) => self.set_text(self.subtitle, &format!("{error} Previous shortcut kept.")),
        }
        self.show_shortcuts();
        self.refresh_status();
    }
    fn command(&mut self, id: usize, notification: u32) {
        if (SHORTCUT_FIRST_ID..SHORTCUT_FIRST_ID + 2).contains(&id) {
            self.cancel_capture();
            self.begin_capture(id - SHORTCUT_FIRST_ID);
            return;
        }
        self.cancel_capture();

        match id {
            PAUSE_ID | TRAY_PAUSE_ID => self.toggle_pause(),
            OPEN_ID => self.open(),
            QUIT_ID => {
                if self.busy || self.update.is_some() {
                    self.exit_requested = true;
                    if let Some(job) = &self.update {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                } else {
                    // SAFETY: only our own root window is destroyed; WM_DESTROY ends the loop.
                    unsafe {
                        DestroyWindow(self.window);
                    }
                }
            }
            2 => {
                // Escape / IDCANCEL hides the settings window.
                // SAFETY: the live root window is retained by the tray application.
                unsafe {
                    ShowWindow(self.window, SW_HIDE);
                }
            }
            INPUT_ID if notification == EN_CHANGE => self.update_preview(),
            STARTUP_ID => {
                // SAFETY: the ID references our checkbox control.
                let checkbox = unsafe { GetDlgItem(self.window, STARTUP_ID as i32) };
                // SAFETY: BM_GETCHECK has no pointer arguments.
                let enabled = unsafe { SendMessageW(checkbox, BM_GETCHECK, 0, 0) } != 0;
                if let Err(error) = settings::set_startup(enabled) {
                    self.notify(&error);
                    // SAFETY: reset the checkbox to the actual registry state after a failed write.
                    unsafe {
                        SendMessageW(
                            checkbox,
                            BM_SETCHECK,
                            usize::from(settings::startup_enabled()),
                            0,
                        );
                    }
                }
            }
            FROM_ID | TO_ID if notification == CBN_SELCHANGE => {
                self.selection = Selection {
                    from: self.selected_layout(self.from),
                    to: self.selected_layout(self.to),
                };
                if let Err(error) = settings::save(settings::Preferences {
                    selection: self.selection,
                    shortcuts: self.shortcuts,
                }) {
                    self.notify(&error);
                }
                self.update_preview();
            }
            ABOUT_ID => {
                if about::show(self.window, self.icon) {
                    self.check_updates();
                }
            }
            UPDATE_ID => self.check_updates(),
            _ => {}
        }
    }
    fn hotkey(&mut self, id: usize) {
        if !self.enabled || self.busy || self.capture.is_some() || self.exit_requested {
            return;
        }
        let action = match id {
            1 => Action::Correct,
            2 => Action::Undo,
            _ => return,
        };
        self.set_text(self.subtitle, "Correcting text...");
        match Target::capture() {
            Ok(target) => {
                if let Some(sender) = &self.work
                    && sender
                        .send(Request {
                            action,
                            target,
                            selection: self.selection,
                            catalog: Arc::clone(&self.catalog),
                        })
                        .is_ok()
                {
                    self.busy = true;
                }
            }
            Err(error) => self.notify(&error),
        }
    }
    fn tick(&mut self) {
        self.finish_capture_if_released();

        if let Some(result) = self
            .results
            .as_ref()
            .and_then(|results| results.try_recv().ok())
        {
            self.busy = false;
            match result {
                Ok(message) => {
                    let message = if message == "Text corrected." {
                        format!("{message} {} undoes the last fix.", self.shortcuts.0[1])
                    } else {
                        message
                    };
                    self.set_text(self.subtitle, &message);
                }
                Err(error) => {
                    self.set_text(self.subtitle, &error);
                    self.notify(&error);
                }
            }
        }
        self.poll_update();
        if self.exit_requested && !self.busy && self.update.is_none() {
            // SAFETY: correction and update workers have finished before the owner closes.
            unsafe {
                DestroyWindow(self.window);
            }
            return;
        }
        self.refresh_tray_language();
    }
    fn start_update(&mut self, update: Option<crate::updates::Update>) {
        if self.update.is_some() || self.exit_requested {
            return;
        }
        let installing = update.is_some();
        let (sender, results) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        match thread::Builder::new()
            .name("github-update".into())
            .spawn(move || {
                let result = match update {
                    Some(update) => {
                        UpdateResult::Installed(updater::install(&update, &worker_cancel))
                    }
                    None => UpdateResult::Checked(updater::check(&worker_cancel)),
                };
                let _ = sender.send(result);
            }) {
            Ok(worker) => {
                self.update = Some(UpdateJob {
                    results,
                    cancel,
                    worker,
                });
                self.set_text(
                    self.subtitle,
                    if installing {
                        "Downloading and verifying update..."
                    } else {
                        "Checking GitHub for updates..."
                    },
                );
            }
            Err(error) => about::update_status(
                self.window,
                &format!("Could not start the update: {error}"),
                true,
            ),
        }
    }
    fn check_updates(&mut self) {
        if self.update.is_some() {
            self.notify("An update check or download is already in progress.");
            return;
        }
        self.start_update(None);
    }
    fn poll_update(&mut self) {
        let result = self
            .update
            .as_ref()
            .and_then(|job| match job.results.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => Some(UpdateResult::Checked(Err(
                    "The update worker stopped unexpectedly.".into(),
                ))),
                Err(mpsc::TryRecvError::Empty) => None,
            });
        let Some(result) = result else {
            return;
        };
        if let Some(job) = self.update.take() {
            let _ = job.worker.join();
        }
        if self.exit_requested {
            return;
        }
        match result {
            UpdateResult::Checked(Ok(Some(update))) => {
                self.set_text(
                    self.subtitle,
                    &format!("Version {} is available.", update.version),
                );
                if about::offer_update(self.window, &update) {
                    self.start_update(Some(update));
                }
            }
            UpdateResult::Checked(Ok(None)) => {
                self.set_text(self.subtitle, "You are up to date.");
                about::update_status(
                    self.window,
                    &format!(
                        "You are up to date. Installed version: {}.",
                        env!("CARGO_PKG_VERSION")
                    ),
                    false,
                );
            }
            UpdateResult::Checked(Err(error)) | UpdateResult::Installed(Err(error)) => {
                self.set_text(
                    self.subtitle,
                    "Update failed. Your current app is unchanged.",
                );
                about::update_status(self.window, &error, true);
            }
            UpdateResult::Installed(Ok(())) => {
                self.set_text(self.subtitle, "Restarting to finish the update...");
                self.exit_requested = true;
            }
        }
    }
    fn refresh_tray_language(&mut self) {
        let language = current_language();
        if language == self.last_language {
            return;
        }
        self.last_language = language;
        let replacement = language_icon(&self.last_language, self.enabled);
        if !replacement.is_null() {
            let previous = self.tray_icon;
            self.tray_icon = replacement;
            if self.tray_added {
                self.update_tray(NIM_MODIFY);
            }
            if !previous.is_null() {
                // SAFETY: the tray has copied the replacement; the previous icon is owned here.
                unsafe {
                    DestroyIcon(previous);
                }
            }
        }
    }
    fn paint(&mut self) {
        // SAFETY: BeginPaint/EndPaint are paired, and all selected GDI objects are restored.
        unsafe {
            let mut paint = PAINTSTRUCT::default();
            let dc = BeginPaint(self.window, &mut paint);
            FillRect(dc, &paint.rcPaint, self.brush(BG));
            let pen = SelectObject(dc, GetStockObject(NULL_PEN));
            for (rect, color) in [([24, 85, 416, 51], GREEN), ([24, 307, 416, 90], WHITE)] {
                let old = SelectObject(dc, self.brush(color));
                RoundRect(
                    dc,
                    self.s(rect[0]),
                    self.s(rect[1]),
                    self.s(rect[0] + rect[2]),
                    self.s(rect[1] + rect[3]),
                    self.s(14),
                    self.s(14),
                );
                SelectObject(dc, old);
            }
            SelectObject(dc, pen);
            EndPaint(self.window, &paint);
        }
    }
    fn color_control(&mut self, dc: HDC, child: HWND) -> LRESULT {
        let colors = self
            .widgets
            .iter()
            .find(|widget| widget.handle == child)
            .map(|widget| (widget.foreground, widget.background))
            .unwrap_or((INK, BG));
        // SAFETY: dc is supplied by WM_CTLCOLOR* and remains valid during this callback.
        unsafe {
            SetTextColor(dc, colors.0);
            SetBkColor(dc, colors.1);
        }
        self.brush(colors.1) as LRESULT
    }
    fn dpi_changed(&mut self, dpi: u32, suggested: *const RECT) {
        self.scale = dpi as f32 / 96.0;
        let old_fonts = std::mem::take(&mut self.fonts);
        self.create_fonts();
        // SAFETY: WM_DPICHANGED provides a live RECT for the callback; child handles are owned.
        unsafe {
            let rect = *suggested;
            SetWindowPos(
                self.window,
                null_mut(),
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            for widget in &self.widgets {
                SetWindowPos(
                    widget.handle,
                    null_mut(),
                    self.s(widget.rect[0]),
                    self.s(widget.rect[1]),
                    self.s(widget.rect[2]),
                    self.s(widget.rect[3]),
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                SendMessageW(
                    widget.handle,
                    WM_SETFONT,
                    self.fonts[widget.font] as usize,
                    1,
                );
            }
            for font in old_fonts {
                DeleteObject(font);
            }
            InvalidateRect(self.window, null(), 1);
        }
    }
    fn tray_menu(&mut self) {
        // SAFETY: the menu is owned locally, used synchronously, then destroyed.
        unsafe {
            let menu = CreatePopupMenu();
            AppendMenuW(
                menu,
                MF_STRING,
                OPEN_ID,
                wide("Open Magic Language Switcher").as_ptr(),
            );
            AppendMenuW(
                menu,
                MF_STRING,
                TRAY_PAUSE_ID,
                wide(if self.enabled {
                    "Pause corrections"
                } else {
                    "Resume corrections"
                })
                .as_ptr(),
            );
            AppendMenuW(
                menu,
                MF_STRING | if self.update.is_some() { MF_GRAYED } else { 0 },
                UPDATE_ID,
                wide(if self.update.is_some() {
                    "Update in progress..."
                } else {
                    "Check for updates"
                })
                .as_ptr(),
            );
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            AppendMenuW(menu, MF_STRING, QUIT_ID, wide("Quit").as_ptr());
            let mut point = POINT::default();
            GetCursorPos(&mut point);
            SetForegroundWindow(self.window);
            let selected = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                self.window,
                null(),
            );
            DestroyMenu(menu);
            PostMessageW(self.window, WM_NULL, 0, 0);
            if selected != 0 {
                self.command(selected as usize, 0);
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.capture_hook.take();
        if let Some(job) = self.update.take() {
            job.cancel.store(true, Ordering::Relaxed);
            let _ = job.worker.join();
        }
        self.work.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        // SAFETY: all handles were created by this app. The message loop has ended.
        unsafe {
            self.registrations.take();
            if self.tray_added {
                Shell_NotifyIconW(NIM_DELETE, &self.tray_data());
            }
            if !self.tray_icon.is_null() {
                DestroyIcon(self.tray_icon);
            }
            if IsWindow(self.window) != 0 {
                // Prevent reentrant messages from borrowing App while its Drop is in progress.
                SetWindowLongPtrW(self.window, GWLP_USERDATA, 0);
                DestroyWindow(self.window);
            }
            for font in &self.fonts {
                DeleteObject(*font);
            }
            for (_, brush) in &self.brushes {
                DeleteObject(*brush);
            }
            if !self.icon.is_null() {
                DestroyIcon(self.icon);
            }
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: WM_NCCREATE passes our stable Box<RefCell<App>> pointer. The box outlives
    // the message loop. try_borrow_mut handles synchronous Win32 reentrancy without aliasing.
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        if message == WM_DESTROY {
            PostQuitMessage(0);
            return 0;
        }
        let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *const RefCell<App>;
        if !pointer.is_null()
            && let Ok(mut app) = (*pointer).try_borrow_mut()
        {
            match message {
                WM_PAINT => {
                    app.paint();
                    return 0;
                }
                WM_ERASEBKGND => return 1,
                WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT => {
                    return app.color_control(wparam as HDC, lparam as HWND);
                }
                WM_COMMAND => {
                    app.command(wparam & 0xffff, (wparam >> 16) as u32);
                    return 0;
                }
                CAPTURE_MESSAGE => {
                    app.capture_key(wparam as u16, (lparam & 0xff) as u16, lparam & 0x100 != 0);
                    return 0;
                }
                WM_HOTKEY => {
                    app.hotkey(wparam);
                    return 0;
                }
                WM_TIMER => {
                    app.tick();
                    return 0;
                }
                WM_CLOSE => {
                    app.cancel_capture();
                    ShowWindow(window, SW_HIDE);
                    return 0;
                }
                WM_ACTIVATE if wparam & 0xffff == WA_INACTIVE as usize => {
                    app.cancel_capture();
                }
                WM_DPICHANGED => {
                    app.dpi_changed(wparam as u32 & 0xffff, lparam as *const RECT);
                    return 0;
                }
                TRAY_MESSAGE => {
                    match lparam as u32 {
                        WM_LBUTTONUP => app.open(),
                        WM_RBUTTONUP | WM_CONTEXTMENU => app.tray_menu(),
                        _ => {}
                    }
                    return 0;
                }
                OPEN_MESSAGE => {
                    app.open();
                    return 0;
                }
                value if value == app.taskbar_message && value != 0 => {
                    app.update_tray(NIM_ADD);
                    return 0;
                }
                _ => {}
            }
        }
        DefWindowProcW(window, message, wparam, lparam)
    }
}

fn language_icon(language: &str, enabled: bool) -> HICON {
    // SAFETY: all GDI handles are local; selected objects are restored before deletion.
    // CreateIconIndirect copies the two bitmaps, so no bitmap escapes this function.
    unsafe {
        let screen = GetDC(null_mut());
        if screen.is_null() {
            return null_mut();
        }
        let dc = CreateCompatibleDC(screen);
        let color = CreateCompatibleBitmap(screen, 32, 32);
        ReleaseDC(null_mut(), screen);
        if dc.is_null() || color.is_null() {
            if !dc.is_null() {
                DeleteDC(dc);
            }
            if !color.is_null() {
                DeleteObject(color);
            }
            return null_mut();
        }
        let old_bitmap = SelectObject(dc, color);
        let background = if !enabled { MUTED } else { BLUE };
        let brush = CreateSolidBrush(background);
        FillRect(
            dc,
            &RECT {
                left: 0,
                top: 0,
                right: 32,
                bottom: 32,
            },
            brush,
        );
        DeleteObject(brush);
        let font = CreateFontW(
            -21,
            0,
            0,
            0,
            700,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            wide("Segoe UI").as_ptr(),
        );
        let old_font = SelectObject(dc, font);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, WHITE);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 32,
            bottom: 32,
        };
        DrawTextW(
            dc,
            wide(language).as_ptr(),
            -1,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(dc, old_font);
        SelectObject(dc, old_bitmap);
        DeleteObject(font);
        DeleteDC(dc);
        let mask = CreateBitmap(32, 32, 1, 1, [0u8; 128].as_ptr() as *const _);
        let info = ICONINFO {
            fIcon: 1,
            hbmColor: color,
            hbmMask: mask,
            ..Default::default()
        };
        let icon = if mask.is_null() {
            null_mut()
        } else {
            CreateIconIndirect(&info)
        };
        DeleteObject(color);
        if !mask.is_null() {
            DeleteObject(mask);
        }
        icon
    }
}

fn current_language() -> String {
    // SAFETY: the locale buffer is stack-allocated and its size is passed to Windows.
    unsafe {
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), null_mut());
        let language = GetKeyboardLayout(thread) as usize as u16;
        let mut buffer = [0u16; 16];
        let length = GetLocaleInfoW(
            language as u32,
            LOCALE_SISO639LANGNAME,
            buffer.as_mut_ptr(),
            buffer.len() as i32,
        );
        if length > 1 {
            String::from_utf16_lossy(&buffer[..length as usize - 1]).to_uppercase()
        } else {
            "??".into()
        }
    }
}

pub(super) fn window_text(window: HWND) -> String {
    // SAFETY: Win32 validates the handle, and the allocation includes room for the terminator.
    unsafe {
        let length = GetWindowTextLengthW(window).max(0) as usize;
        let mut text = vec![0u16; length + 1];
        let copied = GetWindowTextW(window, text.as_mut_ptr(), text.len() as i32).max(0) as usize;
        String::from_utf16_lossy(&text[..copied])
    }
}
fn copy_wide<const N: usize>(target: &mut [u16; N], text: &str) {
    for (slot, value) in target.iter_mut().take(N - 1).zip(text.encode_utf16()) {
        *slot = value;
    }
}

pub fn show_error(error: &str) {
    // SAFETY: both strings are NUL-terminated and live for the synchronous call.
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(error).as_ptr(),
            wide("Magic Language Switcher").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub fn run() -> Result<(), String> {
    if updater::command_mode()? {
        return Ok(());
    }
    // SAFETY: the window classes use static function pointers, App remains boxed until every
    // window is destroyed, and the message loop dispatches only initialized MSG values.
    unsafe {
        let mutex = CreateMutexW(null(), 0, wide("Local\\MagicLanguageSwitcher").as_ptr());
        if mutex.is_null() {
            return Err("Could not create the application instance lock.".into());
        }
        struct MutexGuard(HANDLE);
        impl Drop for MutexGuard {
            fn drop(&mut self) {
                // SAFETY: this owns the one reference returned by CreateMutexW.
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
        let already_running = GetLastError() == ERROR_ALREADY_EXISTS;
        let _mutex = MutexGuard(mutex);
        if already_running {
            let window = FindWindowW(wide(WINDOW_CLASS).as_ptr(), null());
            if !window.is_null() {
                PostMessageW(window, OPEN_MESSAGE, 0, 0);
            }
            return Ok(());
        }
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let common = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES,
        };
        InitCommonControlsEx(&common);
        let instance = GetModuleHandleW(null());
        // Win32 integer resource IDs intentionally occupy the low 16 bits of a pointer.
        let resource_id = std::ptr::without_provenance::<u16>(1);
        let icon = LoadImageW(instance, resource_id, IMAGE_ICON, 0, 0, LR_DEFAULTSIZE) as HICON;
        let root_class = wide(WINDOW_CLASS);
        let root = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: root_class.as_ptr(),
            hIcon: icon,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            ..Default::default()
        };
        if RegisterClassW(&root) == 0 {
            return Err("Could not register the application window.".into());
        }
        let mut area = RECT::default();
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut area as *mut RECT as *mut _, 0);
        let scale = (GetDpiForSystem() as f32 / 96.0)
            .min(((area.bottom - area.top - 64) as f32 / 487.0).max(0.75));
        let app = Box::new(RefCell::new(App::new(instance, icon, scale)));
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
        let mut bounds = RECT {
            right: (464.0 * scale) as i32,
            bottom: (487.0 * scale) as i32,
            ..Default::default()
        };
        AdjustWindowRectExForDpi(&mut bounds, style, 0, 0, (scale * 96.0) as u32);
        let width = bounds.right - bounds.left;
        let height = bounds.bottom - bounds.top;
        let window = CreateWindowExW(
            0,
            root_class.as_ptr(),
            wide("Magic Language Switcher").as_ptr(),
            style,
            area.left + (area.right - area.left - width) / 2,
            area.top + (area.bottom - area.top - height) / 2,
            width,
            height,
            null_mut(),
            null_mut(),
            instance,
            &*app as *const RefCell<App> as *const _,
        );
        if window.is_null() {
            return Err("Could not create the application window.".into());
        }
        {
            let mut state = app.borrow_mut();
            state.window = window;
            state.initialize()?;
        }
        if !std::env::args().any(|arg| arg == "--background") {
            ShowWindow(window, SW_SHOW);
        }
        let mut message = MSG::default();
        loop {
            let result = GetMessageW(&mut message, null_mut(), 0, 0);
            if result == 0 {
                break;
            }
            if result == -1 {
                return Err("The Windows message loop failed.".into());
            }
            let keyboard_message = (WM_KEYFIRST..=WM_KEYLAST).contains(&message.message);
            if keyboard_message && app.borrow().capture.is_some() {
                continue;
            }
            if !keyboard_message || IsDialogMessageW(window, &message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        drop(app);
        Ok(())
    }
}
