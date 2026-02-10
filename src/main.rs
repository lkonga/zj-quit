use zellij_tile::prelude::*;

use std::collections::BTreeMap;

/// The action to perform after confirmation
#[derive(Debug, Clone, PartialEq)]
enum Action {
    /// Quit the entire zellij session (original behavior)
    QuitSession,
    /// Close the focused pane
    ClosePane,
    /// Close the focused tab
    CloseTab,
}

impl Default for Action {
    fn default() -> Self {
        Action::QuitSession
    }
}

impl Action {
    fn from_config(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "close_pane" | "closepane" | "pane" => Action::ClosePane,
            "close_tab" | "closetab" | "tab" => Action::CloseTab,
            "quit" | "quit_session" | "session" | _ => Action::QuitSession,
        }
    }

    fn confirmation_text(&self) -> &'static str {
        match self {
            Action::QuitSession => "Are you sure you want to quit this session?",
            Action::ClosePane => "Are you sure you want to close this pane?",
            Action::CloseTab => "Are you sure you want to close this tab?",
        }
    }

    fn action_name(&self) -> &'static str {
        match self {
            Action::QuitSession => "Quit Session",
            Action::ClosePane => "Close Pane",
            Action::CloseTab => "Close Tab",
        }
    }
}

struct State {
    confirm_key: KeyWithModifier,
    cancel_key: KeyWithModifier,
    action: Action,
    /// The pane that was focused before the plugin opened (for ClosePane action)
    target_pane_id: Option<PaneId>,
    /// The tab index that was focused before the plugin opened (for CloseTab action)
    target_tab_index: Option<usize>,
    /// Whether we've received pane info yet
    pane_info_received: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            confirm_key: KeyWithModifier::new(BareKey::Enter),
            cancel_key: KeyWithModifier::new(BareKey::Esc),
            action: Action::default(),
            target_pane_id: None,
            target_tab_index: None,
            pane_info_received: false,
        }
    }
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        request_permission(&[
            PermissionType::ChangeApplicationState,
            PermissionType::ReadApplicationState,
        ]);
        subscribe(&[EventType::Key, EventType::PaneUpdate, EventType::TabUpdate]);

        // Parse confirm key
        if let Some(confirm_key) = configuration.get("confirm_key") {
            self.confirm_key = confirm_key.parse().unwrap_or(self.confirm_key.clone());
        }

        // Parse cancel key
        if let Some(abort_key) = configuration.get("cancel_key") {
            self.cancel_key = abort_key.parse().unwrap_or(self.cancel_key.clone());
        }

        // Parse action from configuration
        if let Some(action_str) = configuration.get("action") {
            self.action = Action::from_config(action_str);
        }
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::Key(key) => {
                if self.confirm_key == key {
                    self.execute_action();
                } else if self.cancel_key == key {
                    hide_self();
                }
            }
            Event::PaneUpdate(pane_manifest) => {
                // Only capture the target pane once (when plugin first opens)
                if !self.pane_info_received && self.action == Action::ClosePane {
                    // Find the focused non-plugin pane in the current tab
                    if let Some(tab_index) = self.target_tab_index {
                        if let Some(pane_info) = get_focused_pane(tab_index, &pane_manifest) {
                            self.target_pane_id = if pane_info.is_plugin {
                                Some(PaneId::Plugin(pane_info.id))
                            } else {
                                Some(PaneId::Terminal(pane_info.id))
                            };
                        }
                    }
                    self.pane_info_received = true;
                }
            }
            Event::TabUpdate(tab_infos) => {
                // Capture the focused tab index when plugin opens
                if self.target_tab_index.is_none() {
                    if let Some(focused_tab) = get_focused_tab(&tab_infos) {
                        self.target_tab_index = Some(focused_tab.position);
                    }
                }
            }
            _ => (),
        };
        // Return true to trigger re-render after receiving pane/tab info
        true
    }

    fn render(&mut self, rows: usize, cols: usize) {
        // Title line showing what action will be performed
        let title = format!("[ {} ]", self.action.action_name());
        let title_y = (rows / 2) - 3;
        let title_x = cols.saturating_sub(title.chars().count()) / 2;

        print_text_with_coordinates(
            Text::new(&title).color_range(2, 0..title.chars().count()),
            title_x,
            title_y,
            None,
            None,
        );

        // Confirmation text
        let confirmation_text = self.action.confirmation_text().to_string();
        let confirmation_y_location = (rows / 2) - 1;
        let confirmation_x_location = cols.saturating_sub(confirmation_text.chars().count()) / 2;

        print_text_with_coordinates(
            Text::new(confirmation_text),
            confirmation_x_location,
            confirmation_y_location,
            None,
            None,
        );

        // Show target info for pane/tab close
        let target_info = match self.action {
            Action::ClosePane => {
                if let Some(pane_id) = &self.target_pane_id {
                    match pane_id {
                        PaneId::Terminal(id) => format!("Target: Terminal pane #{}", id),
                        PaneId::Plugin(id) => format!("Target: Plugin pane #{}", id),
                    }
                } else {
                    "Target: (detecting...)".to_string()
                }
            }
            Action::CloseTab => {
                if let Some(tab_idx) = self.target_tab_index {
                    format!("Target: Tab #{}", tab_idx + 1)
                } else {
                    "Target: (detecting...)".to_string()
                }
            }
            Action::QuitSession => String::new(),
        };

        if !target_info.is_empty() {
            let info_y = (rows / 2) + 1;
            let info_x = cols.saturating_sub(target_info.chars().count()) / 2;
            print_text_with_coordinates(
                Text::new(target_info).color_range(1, 0..7), // Color "Target:" prefix
                info_x,
                info_y,
                None,
                None,
            );
        }

        // Help text at bottom
        let help_text = format!(
            "Help: <{}> - Confirm, <{}> - Cancel",
            self.confirm_key, self.cancel_key,
        );
        let help_text_y_location = rows - 1;
        let help_text_x_location = cols.saturating_sub(help_text.chars().count()) / 2;

        let confirm_key_length = self.confirm_key.to_string().chars().count();
        let abort_key_length = self.cancel_key.to_string().chars().count();

        print_text_with_coordinates(
            Text::new(help_text)
                .color_range(3, 6..8 + confirm_key_length)
                .color_range(
                    3,
                    20 + confirm_key_length..22 + confirm_key_length + abort_key_length,
                ),
            help_text_x_location,
            help_text_y_location,
            None,
            None,
        );
    }
}

impl State {
    fn execute_action(&self) {
        match self.action {
            Action::QuitSession => {
                quit_zellij();
            }
            Action::ClosePane => {
                // First hide ourselves, then close the target pane
                hide_self();
                if let Some(pane_id) = &self.target_pane_id {
                    close_pane_with_id(pane_id.clone());
                } else {
                    // Fallback: if we couldn't identify the pane, use close_focus
                    // This might close the plugin itself, but it's better than nothing
                    close_focus();
                }
            }
            Action::CloseTab => {
                // First hide ourselves, then close the tab
                hide_self();
                close_focused_tab();
            }
        }
    }
}
