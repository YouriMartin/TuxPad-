// main.rs – TuxPad++ entry point
//
// Creates the Adwaita application, builds the main window with:
//   • an Adwaita HeaderBar with a hamburger menu,
//   • a quick-search bar (activated with Ctrl+F),
//   • a split-pane grid (auto-layout: 1 pane full, 2 side-by-side, 3+ in rows of 2),
//   • a status bar showing cursor position.
//
// Vertical separators between panes in the same row have a chain-link toggle
// button that synchronises the scroll of the two adjacent panes.

mod diff;
mod editor;
mod formatter;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use adw::prelude::*;
use sourceview5::prelude::*;

const APP_ID: &str = "com.tuxpad.TuxPad";

// ─── Types ───────────────────────────────────────────────────────────────────

/// A list of (Adjustment, SignalHandlerId) pairs used to disconnect scroll sync.
type HandlerList = Rc<RefCell<Vec<(gtk4::Adjustment, glib::SignalHandlerId)>>>;

/// Shared mutable state for the split-pane layout.
type State = Rc<RefCell<SplitState>>;

struct PaneData {
    /// Outer container widget: pane header + scrolled window.
    root: gtk4::Box,
    /// The scrolled window (kept for scroll-sync access).
    scrolled: gtk4::ScrolledWindow,
    /// The underlying editor view.
    editor: editor::EditorView,
    /// Filename label shown in the pane header.
    label: gtk4::Label,
}

struct SplitState {
    panes: Vec<PaneData>,
    /// Root widget of the currently focused pane (used by actions).
    active: Option<gtk4::Box>,
    /// One HandlerList per currently linked separator.
    active_handler_lists: Vec<HandlerList>,
}

// ─── Application entry point ─────────────────────────────────────────────────

fn main() -> glib::ExitCode {
    let app = adw::Application::new(Some(APP_ID), gio::ApplicationFlags::empty());
    app.connect_activate(build_ui);
    app.run()
}

// ─── UI builder ──────────────────────────────────────────────────────────────

fn build_ui(app: &adw::Application) {
    sourceview5::init();
    let window = build_main_window(app);
    window.present();
}

fn build_main_window(app: &adw::Application) -> adw::ApplicationWindow {
    // ── Window ----------------------------------------------------------------
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("TuxPad++")
        .default_width(1100)
        .default_height(780)
        .build();

    // ── Root layout -----------------------------------------------------------
    let root_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    // ── Header bar ------------------------------------------------------------
    let header_bar = adw::HeaderBar::new();

    let new_pane_button = gtk4::Button::new();
    new_pane_button.set_icon_name("tab-new-symbolic");
    new_pane_button.set_tooltip_text(Some("New Pane (Ctrl+T)"));
    header_bar.pack_start(&new_pane_button);

    let menu_button = gtk4::MenuButton::new();
    menu_button.set_icon_name("open-menu-symbolic");
    let menu_model = build_app_menu();
    menu_button.set_menu_model(Some(&menu_model));
    header_bar.pack_end(&menu_button);

    root_box.append(&header_bar);

    // ── Search bar (shown with Ctrl+F) ----------------------------------------
    let search_bar = gtk4::SearchBar::new();
    search_bar.set_show_close_button(true);

    let search_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    search_row.set_margin_start(8);
    search_row.set_margin_end(8);

    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_hexpand(true);
    search_entry.set_placeholder_text(Some("Search… (regex supported)"));

    let prev_button = gtk4::Button::new();
    prev_button.set_icon_name("go-up-symbolic");
    prev_button.set_tooltip_text(Some("Previous match"));

    let next_button = gtk4::Button::new();
    next_button.set_icon_name("go-down-symbolic");
    next_button.set_tooltip_text(Some("Next match"));

    let match_label = gtk4::Label::new(Some(""));

    search_row.append(&search_entry);
    search_row.append(&prev_button);
    search_row.append(&next_button);
    search_row.append(&match_label);
    search_bar.set_child(Some(&search_row));
    search_bar.connect_entry(&search_entry);

    root_box.append(&search_bar);

    // ── Split-pane container --------------------------------------------------
    let outer_panes_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    outer_panes_box.set_vexpand(true);
    outer_panes_box.set_hexpand(true);

    root_box.append(&outer_panes_box);

    // ── Status bar ------------------------------------------------------------
    let status_bar = gtk4::Label::new(Some("Ln 1, Col 1"));
    status_bar.set_xalign(0.0);
    status_bar.set_margin_start(8);
    status_bar.set_margin_end(8);
    status_bar.set_margin_top(3);
    status_bar.set_margin_bottom(3);

    let status_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    status_box.add_css_class("statusbar");
    status_box.append(&status_bar);
    root_box.append(&status_box);

    // ── Assemble window -------------------------------------------------------
    window.set_content(Some(&root_box));

    // ── Initial split state ---------------------------------------------------
    let state: State = Rc::new(RefCell::new(SplitState {
        panes: Vec::new(),
        active: None,
        active_handler_lists: Vec::new(),
    }));

    // Open with one blank pane
    add_pane(&outer_panes_box, &state, &status_bar, None);

    // ── Keyboard shortcuts ----------------------------------------------------
    setup_shortcuts(&window, &outer_panes_box, &state, &status_bar, &search_bar);

    // ── Actions ---------------------------------------------------------------
    setup_actions(
        &window,
        &outer_panes_box,
        &state,
        &status_bar,
        &search_entry,
        &match_label,
    );

    // Connect new-pane button to win.new_tab action
    new_pane_button.connect_clicked({
        let window = window.clone();
        move |_| {
            if let Some(action) = window.lookup_action("new_tab") {
                action.activate(None);
            }
        }
    });

    window
}

// ─── Menu model ──────────────────────────────────────────────────────────────

fn build_app_menu() -> gio::Menu {
    let menu = gio::Menu::new();

    let file_section = gio::Menu::new();
    file_section.append(Some("New Pane"), Some("win.new_tab"));
    file_section.append(Some("Open File…"), Some("win.open_file"));
    file_section.append(Some("Save"), Some("win.save_file"));
    file_section.append(Some("Save As…"), Some("win.save_file_as"));
    menu.append_section(Some("File"), &file_section);

    let tools_section = gio::Menu::new();
    tools_section.append(Some("Format Code (Beautify)"), Some("win.format_code"));
    tools_section.append(Some("Show Diff…"), Some("win.show_diff"));
    menu.append_section(Some("Tools"), &tools_section);

    let app_section = gio::Menu::new();
    app_section.append(Some("About TuxPad++"), Some("app.about"));
    menu.append_section(None, &app_section);

    menu
}

// ─── Pane helpers ─────────────────────────────────────────────────────────────

/// Create a new `PaneData` for a single editor pane.
///
/// Builds the header (label + close button), wraps the editor in a vertical Box,
/// and connects the status-bar cursor signal and focus tracking.
fn create_pane_data(
    outer_box: &gtk4::Box,
    state: &State,
    status_bar: &gtk4::Label,
    file_path: Option<&std::path::Path>,
) -> PaneData {
    let mut ev = editor::EditorView::new();

    let label_text = if let Some(path) = file_path {
        match ev.open_file(path) {
            Ok(()) => {
                unsafe { ev.view().set_data("file_path", path.to_path_buf()); }
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Untitled")
                    .to_owned()
            }
            Err(e) => {
                eprintln!("Error opening file: {}", e);
                "Untitled".to_owned()
            }
        }
    } else {
        "Untitled".to_owned()
    };

    // Header
    let label = gtk4::Label::new(Some(&label_text));
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label.set_max_width_chars(20);
    label.set_hexpand(true);
    label.set_xalign(0.0);

    let close_btn = gtk4::Button::new();
    close_btn.set_icon_name("window-close-symbolic");
    close_btn.set_has_frame(false);
    close_btn.add_css_class("flat");
    close_btn.set_tooltip_text(Some("Close pane"));

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    header.set_margin_start(4);
    header.set_margin_end(4);
    header.set_margin_top(2);
    header.set_margin_bottom(2);
    header.append(&label);
    header.append(&close_btn);

    // Pane root
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.set_hexpand(true);
    root.set_vexpand(true);
    root.append(&header);
    root.append(ev.widget());

    let scrolled = ev.widget().clone();

    // Close button → remove_pane
    {
        let ob = outer_box.clone();
        let st = state.clone();
        let root_clone = root.clone();
        close_btn.connect_clicked(move |_| {
            remove_pane(&ob, &st, &root_clone);
        });
    }

    // Status bar update on cursor move
    {
        let status = status_bar.clone();
        ev.view().buffer().connect_mark_set(move |buf, iter, mark| {
            if mark.name().as_deref() == Some("insert") {
                let line = iter.line() + 1;
                let col = buf
                    .iter_at_line_offset(iter.line(), 0)
                    .map(|start| iter.offset() - start.offset() + 1)
                    .unwrap_or(1);
                status.set_text(&format!("Ln {}, Col {}", line, col));
            }
        });
    }

    // Track active pane when the editor gains focus
    {
        let st = state.clone();
        let root_clone = root.clone();
        ev.view().connect_has_focus_notify(move |view| {
            if view.has_focus() {
                st.borrow_mut().active = Some(root_clone.clone());
            }
        });
    }

    PaneData { root, scrolled, editor: ev, label }
}

/// Add a new pane to the layout, optionally opening `file_path`.
fn add_pane(
    outer_box: &gtk4::Box,
    state: &State,
    status_bar: &gtk4::Label,
    file_path: Option<&std::path::Path>,
) {
    let pane = create_pane_data(outer_box, state, status_bar, file_path);
    {
        let mut s = state.borrow_mut();
        let root = pane.root.clone();
        s.panes.push(pane);
        s.active = Some(root);
    }
    rebuild_layout(outer_box, state);
}

/// Remove the pane whose root matches `root`. Guards against closing the last pane.
fn remove_pane(outer_box: &gtk4::Box, state: &State, root: &gtk4::Box) {
    let (panes_len, idx) = {
        let s = state.borrow();
        let len = s.panes.len();
        let idx = s.panes.iter().position(|p| p.root == *root);
        (len, idx)
    };

    if panes_len <= 1 {
        return; // Keep at least one pane
    }

    if let Some(idx) = idx {
        let new_active_idx = if idx > 0 { idx - 1 } else { 1 };
        let new_active = state.borrow().panes[new_active_idx].root.clone();
        {
            let mut s = state.borrow_mut();
            s.active = Some(new_active);
            s.panes.remove(idx);
        }
        rebuild_layout(outer_box, state);
    }
}

/// Disconnect all scroll-sync handlers, clear the container, and rebuild the
/// grid layout from the current pane list in chunks of two per row.
fn rebuild_layout(outer_box: &gtk4::Box, state: &State) {
    // 1. Disconnect all scroll-sync handlers
    {
        let mut s = state.borrow_mut();
        for hl in s.active_handler_lists.drain(..) {
            for (adj, id) in hl.borrow_mut().drain(..) {
                adj.disconnect(id);
            }
        }
    }

    // 2. Unparent all pane roots from their current row_boxes
    {
        let s = state.borrow();
        for pane in &s.panes {
            if let Some(parent) = pane.root.parent() {
                if let Ok(parent_box) = parent.downcast::<gtk4::Box>() {
                    parent_box.remove(&pane.root);
                }
            }
        }
    }

    // 3. Clear outer_box (now only contains empty row_boxes / row separators)
    while let Some(child) = outer_box.first_child() {
        outer_box.remove(&child);
    }

    // 4. Rebuild grid in chunks of 2
    let panes_info: Vec<(gtk4::Box, gtk4::ScrolledWindow)> = {
        let s = state.borrow();
        s.panes.iter().map(|p| (p.root.clone(), p.scrolled.clone())).collect()
    };

    let n = panes_info.len();
    let mut i = 0;
    let mut first_row = true;

    while i < n {
        if !first_row {
            let hsep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
            outer_box.append(&hsep);
        }
        first_row = false;

        let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        row_box.set_vexpand(true);
        row_box.set_hexpand(true);

        let (root_a, sw_a) = panes_info[i].clone();
        root_a.set_hexpand(true);

        if i + 1 < n {
            let (root_b, sw_b) = panes_info[i + 1].clone();
            root_b.set_hexpand(true);

            let sep_widget = build_separator_widget(&sw_a, &sw_b, state);

            row_box.append(&root_a);
            row_box.append(&sep_widget);
            row_box.append(&root_b);

            i += 2;
        } else {
            row_box.append(&root_a);
            i += 1;
        }

        outer_box.append(&row_box);
    }
}

/// Build the 16 px-wide separator+chain-button widget placed between two panes
/// in the same row.
///
/// The toggle button, when activated, creates four scroll-sync signal handlers
/// (left↔right for both vertical and horizontal adjustments). Deactivating it
/// tears them all down again.
fn build_separator_widget(
    left_sw: &gtk4::ScrolledWindow,
    right_sw: &gtk4::ScrolledWindow,
    state: &State,
) -> gtk4::Box {
    let sep_container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    sep_container.set_size_request(16, -1);
    sep_container.set_vexpand(true);

    let overlay = gtk4::Overlay::new();
    overlay.set_vexpand(true);
    overlay.set_hexpand(true);

    let vsep = gtk4::Separator::new(gtk4::Orientation::Vertical);
    vsep.set_vexpand(true);
    overlay.set_child(Some(&vsep));

    let chain_btn = gtk4::ToggleButton::new();
    chain_btn.set_icon_name("insert-link-symbolic");
    chain_btn.set_tooltip_text(Some("Link scroll (sync vertical & horizontal)"));
    chain_btn.set_halign(gtk4::Align::Center);
    chain_btn.set_valign(gtk4::Align::Center);
    overlay.add_overlay(&chain_btn);

    sep_container.append(&overlay);

    // Per-button handler list (None while unlinked, Some while linked)
    let active_hl: Rc<RefCell<Option<HandlerList>>> = Rc::new(RefCell::new(None));

    let left_sw_c = left_sw.clone();
    let right_sw_c = right_sw.clone();
    let state_c = state.clone();

    chain_btn.connect_toggled(move |btn| {
        if btn.is_active() {
            // ── Link: connect four bidirectional scroll handlers ──────────────
            let left_vadj = left_sw_c.vadjustment();
            let right_vadj = right_sw_c.vadjustment();
            let left_hadj = left_sw_c.hadjustment();
            let right_hadj = right_sw_c.hadjustment();

            let guard_v = Rc::new(Cell::new(false));
            let guard_h = Rc::new(Cell::new(false));

            let hl: HandlerList = Rc::new(RefCell::new(Vec::new()));

            // left_v → right_v
            {
                let guard = guard_v.clone();
                let right = right_vadj.clone();
                let id = left_vadj.connect_value_changed(move |adj| {
                    if guard.get() { return; }
                    guard.set(true);
                    right.set_value(adj.value());
                    guard.set(false);
                });
                hl.borrow_mut().push((left_vadj.clone(), id));
            }

            // right_v → left_v
            {
                let guard = guard_v.clone();
                let left = left_vadj.clone();
                let id = right_vadj.connect_value_changed(move |adj| {
                    if guard.get() { return; }
                    guard.set(true);
                    left.set_value(adj.value());
                    guard.set(false);
                });
                hl.borrow_mut().push((right_vadj.clone(), id));
            }

            // left_h → right_h
            {
                let guard = guard_h.clone();
                let right = right_hadj.clone();
                let id = left_hadj.connect_value_changed(move |adj| {
                    if guard.get() { return; }
                    guard.set(true);
                    right.set_value(adj.value());
                    guard.set(false);
                });
                hl.borrow_mut().push((left_hadj.clone(), id));
            }

            // right_h → left_h
            {
                let guard = guard_h.clone();
                let left = left_hadj.clone();
                let id = right_hadj.connect_value_changed(move |adj| {
                    if guard.get() { return; }
                    guard.set(true);
                    left.set_value(adj.value());
                    guard.set(false);
                });
                hl.borrow_mut().push((right_hadj.clone(), id));
            }

            // Store the list so we can find it on unlink
            *active_hl.borrow_mut() = Some(hl.clone());
            state_c.borrow_mut().active_handler_lists.push(hl);
        } else {
            // ── Unlink: disconnect handlers and remove from state ─────────────
            if let Some(hl) = active_hl.borrow_mut().take() {
                for (adj, id) in hl.borrow_mut().drain(..) {
                    adj.disconnect(id);
                }
                state_c
                    .borrow_mut()
                    .active_handler_lists
                    .retain(|h| !Rc::ptr_eq(h, &hl));
            }
        }
    });

    sep_container
}

/// Return the `sourceview5::View` of the currently active pane, if any.
fn current_view(state: &State) -> Option<sourceview5::View> {
    let s = state.borrow();
    let active_root = s.active.as_ref()?;
    s.panes
        .iter()
        .find(|p| p.root == *active_root)
        .map(|p| p.editor.view().clone())
}

// ─── Keyboard shortcuts ───────────────────────────────────────────────────────

fn setup_shortcuts(
    window: &adw::ApplicationWindow,
    outer_box: &gtk4::Box,
    state: &State,
    status_bar: &gtk4::Label,
    search_bar: &gtk4::SearchBar,
) {
    // Ctrl+F – toggle search bar
    {
        let sb = search_bar.clone();
        let ctrl_f = gtk4::ShortcutController::new();
        let trigger = gtk4::KeyvalTrigger::new(
            gtk4::gdk::Key::f,
            gtk4::gdk::ModifierType::CONTROL_MASK,
        );
        let action = gtk4::CallbackAction::new(move |_, _| {
            let active = !sb.is_search_mode();
            sb.set_search_mode(active);
            glib::Propagation::Stop
        });
        ctrl_f.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        window.add_controller(ctrl_f);
    }

    // Ctrl+T – new pane
    {
        let ob = outer_box.clone();
        let st = state.clone();
        let sb = status_bar.clone();
        let ctrl_t = gtk4::ShortcutController::new();
        let trigger = gtk4::KeyvalTrigger::new(
            gtk4::gdk::Key::t,
            gtk4::gdk::ModifierType::CONTROL_MASK,
        );
        let action = gtk4::CallbackAction::new(move |_, _| {
            add_pane(&ob, &st, &sb, None);
            glib::Propagation::Stop
        });
        ctrl_t.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        window.add_controller(ctrl_t);
    }

    // Ctrl+S – save current file
    {
        let st = state.clone();
        let ctrl_s = gtk4::ShortcutController::new();
        let trigger = gtk4::KeyvalTrigger::new(
            gtk4::gdk::Key::s,
            gtk4::gdk::ModifierType::CONTROL_MASK,
        );
        let action = gtk4::CallbackAction::new(move |_, _| {
            if let Some(view) = current_view(&st) {
                if let Some(path_ptr) =
                    unsafe { view.data::<std::path::PathBuf>("file_path") }
                {
                    let path = unsafe { path_ptr.as_ref().clone() };
                    let buf = view.buffer();
                    let start = buf.start_iter();
                    let end = buf.end_iter();
                    let text = buf.text(&start, &end, true);
                    if let Err(e) = std::fs::write(&path, text.as_bytes()) {
                        eprintln!("Save error: {}", e);
                    }
                }
            }
            glib::Propagation::Stop
        });
        ctrl_s.add_shortcut(gtk4::Shortcut::new(Some(trigger), Some(action)));
        window.add_controller(ctrl_s);
    }
}

// ─── Actions ─────────────────────────────────────────────────────────────────

fn setup_actions(
    window: &adw::ApplicationWindow,
    outer_box: &gtk4::Box,
    state: &State,
    status_bar: &gtk4::Label,
    search_entry: &gtk4::SearchEntry,
    match_label: &gtk4::Label,
) {
    // win.new_tab
    {
        let ob = outer_box.clone();
        let st = state.clone();
        let sb = status_bar.clone();
        let action = gio::SimpleAction::new("new_tab", None);
        action.connect_activate(move |_, _| {
            add_pane(&ob, &st, &sb, None);
        });
        window.add_action(&action);
    }

    // win.open_file
    {
        let ob = outer_box.clone();
        let st = state.clone();
        let sb = status_bar.clone();
        let win = window.clone();
        let action = gio::SimpleAction::new("open_file", None);
        action.connect_activate(move |_, _| {
            let dialog = gtk4::FileDialog::builder()
                .title("Open File")
                .modal(true)
                .build();
            let ob2 = ob.clone();
            let st2 = st.clone();
            let sb2 = sb.clone();
            dialog.open(
                Some(&win),
                None::<&gio::Cancellable>,
                move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            add_pane(&ob2, &st2, &sb2, Some(&path));
                        }
                    }
                },
            );
        });
        window.add_action(&action);
    }

    // win.save_file
    {
        let st = state.clone();
        let action = gio::SimpleAction::new("save_file", None);
        action.connect_activate(move |_, _| {
            if let Some(view) = current_view(&st) {
                if let Some(path_ptr) =
                    unsafe { view.data::<std::path::PathBuf>("file_path") }
                {
                    let path = unsafe { path_ptr.as_ref().clone() };
                    let buf = view.buffer();
                    let start = buf.start_iter();
                    let end = buf.end_iter();
                    let text = buf.text(&start, &end, true);
                    if let Err(e) = std::fs::write(&path, text.as_bytes()) {
                        eprintln!("Save error: {}", e);
                    }
                }
            }
        });
        window.add_action(&action);
    }

    // win.save_file_as
    {
        let st = state.clone();
        let win = window.clone();
        let action = gio::SimpleAction::new("save_file_as", None);
        action.connect_activate(move |_, _| {
            if let Some(view) = current_view(&st) {
                let dialog = gtk4::FileDialog::builder()
                    .title("Save As")
                    .modal(true)
                    .build();
                let view_clone = view.clone();
                dialog.save(
                    Some(&win),
                    None::<&gio::Cancellable>,
                    move |result| {
                        if let Ok(file) = result {
                            if let Some(path) = file.path() {
                                let buf = view_clone.buffer();
                                let start = buf.start_iter();
                                let end = buf.end_iter();
                                let text = buf.text(&start, &end, true);
                                if std::fs::write(&path, text.as_bytes()).is_ok() {
                                    unsafe {
                                        view_clone.set_data("file_path", path.clone());
                                    }
                                } else {
                                    eprintln!(
                                        "Save As error: could not write to {}",
                                        path.display()
                                    );
                                }
                            }
                        }
                    },
                );
            }
        });
        window.add_action(&action);
    }

    // win.format_code – run the appropriate external formatter
    {
        let st = state.clone();
        let win = window.clone();
        let action = gio::SimpleAction::new("format_code", None);
        action.connect_activate(move |_, _| {
            let Some(view) = current_view(&st) else {
                return;
            };

            let language_id: Option<String> = view
                .buffer()
                .downcast::<sourceview5::Buffer>()
                .ok()
                .and_then(|b| b.language())
                .map(|l: sourceview5::Language| l.id().to_string())
                .or_else(|| {
                    let path = unsafe { view.data::<std::path::PathBuf>("file_path") }
                        .map(|ptr| unsafe { ptr.as_ref().clone() })?;
                    let name = path.file_name()?.to_str()?;
                    let lm = sourceview5::LanguageManager::default();
                    lm.guess_language(Some(name), None)
                        .map(|l| l.id().to_string())
                });

            let Some(lang_id) = language_id else {
                show_error_dialog(
                    &win,
                    "Language not detected",
                    "Could not determine the language for this pane.\n\
                     Save the file with the correct extension (e.g. .json, .rs, .py) and try again.",
                );
                return;
            };

            let Some(fmt) = formatter::Formatter::for_language(&lang_id) else {
                show_error_dialog(
                    &win,
                    "No formatter available",
                    &format!(
                        "No formatter is configured for '{lang_id}'.\n\
                         Supported languages: Rust (rustfmt), JS/TS/CSS/HTML/JSON (prettier), \
                         Python (black), C/C++ (clang-format)."
                    ),
                );
                return;
            };

            if let Some(path_ptr) =
                unsafe { view.data::<std::path::PathBuf>("file_path") }
            {
                let path = unsafe { path_ptr.as_ref().clone() };
                let buf = view.buffer();
                let start = buf.start_iter();
                let end = buf.end_iter();
                let text = buf.text(&start, &end, true);
                if std::fs::write(&path, text.as_bytes()).is_ok() {
                    match fmt.format_file(&path) {
                        Ok(()) => {
                            if let Ok(formatted) = std::fs::read_to_string(&path) {
                                buf.set_text(&formatted);
                            }
                        }
                        Err(e) => show_error_dialog(&win, "Format error", &e),
                    }
                }
            } else {
                show_error_dialog(
                    &win,
                    "File not saved",
                    "Please save the file before formatting.",
                );
            }
        });
        window.add_action(&action);
    }

    // win.show_diff – compare saved version with current editor content
    {
        let st = state.clone();
        let win = window.clone();
        let action = gio::SimpleAction::new("show_diff", None);
        action.connect_activate(move |_, _| {
            let Some(view) = current_view(&st) else {
                return;
            };

            let buf = view.buffer();
            let start = buf.start_iter();
            let end = buf.end_iter();
            let current_text = buf.text(&start, &end, true).to_string();

            let saved_text = unsafe { view.data::<std::path::PathBuf>("file_path") }
                .map(|ptr| unsafe { ptr.as_ref().clone() })
                .and_then(|path| std::fs::read_to_string(path).ok())
                .unwrap_or_default();

            diff::show_diff_dialog(&win, &saved_text, &current_text);
        });
        window.add_action(&action);
    }

    // Search entry: activate on Enter – case-insensitive, all panes
    {
        let st = state.clone();
        let ml = match_label.clone();
        let se = search_entry.clone();
        search_entry.connect_activate(move |_| {
            let pattern = se.text().to_string();
            if pattern.is_empty() {
                return;
            }
            // (?i) makes the match case-insensitive
            let ci_pattern = format!("(?i){}", pattern);
            match regex::Regex::new(&ci_pattern) {
                Ok(re) => {
                    // Sum matches across every open pane
                    let views: Vec<sourceview5::View> = st
                        .borrow()
                        .panes
                        .iter()
                        .map(|p| p.editor.view().clone())
                        .collect();
                    let total: usize = views
                        .iter()
                        .map(|view| {
                            let buf = view.buffer();
                            let text = buf
                                .text(&buf.start_iter(), &buf.end_iter(), true)
                                .to_string();
                            re.find_iter(&text).count()
                        })
                        .sum();
                    ml.set_text(&format!(
                        "{} match{}",
                        total,
                        if total == 1 { "" } else { "es" }
                    ));
                }
                Err(_) => ml.set_text("invalid regex"),
            }
        });
    }

    // Search entry: live highlight as user types – case-insensitive, all panes
    {
        let st = state.clone();
        search_entry.connect_search_changed(move |entry| {
            let pattern = entry.text().to_string();
            let ci_pattern = if pattern.is_empty() {
                String::new()
            } else {
                format!("(?i){}", pattern)
            };

            // Apply to every pane
            let views: Vec<sourceview5::View> = st
                .borrow()
                .panes
                .iter()
                .map(|p| p.editor.view().clone())
                .collect();

            for view in &views {
                let buf = match view.buffer().downcast::<sourceview5::Buffer>() {
                    Ok(b) => b,
                    Err(_) => continue,
                };

                // Always clear old highlights first
                buf.remove_tag_by_name(
                    "search-highlight",
                    &buf.start_iter(),
                    &buf.end_iter(),
                );

                if ci_pattern.is_empty() {
                    continue;
                }

                // Ensure the highlight tag exists in this buffer
                if buf.tag_table().lookup("search-highlight").is_none() {
                    let tag = gtk4::TextTag::new(Some("search-highlight"));
                    tag.set_background(Some("#f39c12"));
                    tag.set_foreground(Some("#000000"));
                    buf.tag_table().add(&tag);
                }

                if let Ok(re) = regex::Regex::new(&ci_pattern) {
                    let text = buf
                        .text(&buf.start_iter(), &buf.end_iter(), true)
                        .to_string();
                    for m in re.find_iter(&text) {
                        let sc = text[..m.start()].chars().count() as i32;
                        let ec = text[..m.end()].chars().count() as i32;
                        buf.apply_tag_by_name(
                            "search-highlight",
                            &buf.iter_at_offset(sc),
                            &buf.iter_at_offset(ec),
                        );
                    }
                }
            }
        });
    }
}

// ─── Error dialog helper ──────────────────────────────────────────────────────

fn show_error_dialog(window: &adw::ApplicationWindow, title: &str, message: &str) {
    let dialog = adw::AlertDialog::new(Some(title), Some(message));
    dialog.add_response("ok", "OK");
    dialog.present(Some(window));
}
