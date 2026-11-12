//! Group sorting for the CONFIGS list.

use npvt_core::profile::Profile;

use crate::app::App;
use crate::settings::SortMode;
use crate::vault_actions;

/// Return the (label, members) pairs in the configured display order.
pub fn sorted_groups(app: &App) -> Vec<(String, Vec<Profile>)> {
    let mut groups = vault_actions::group_profiles(&app.vault);
    for label in &app.settings.extra_groups {
        if !groups.iter().any(|(l, _)| l == label) {
            groups.push((label.clone(), Vec::new()));
        }
    }
    match app.settings.sort {
        SortMode::Name => {
            groups.sort_by_key(|(label, _)| label.to_lowercase());
        }
        SortMode::Ping => {
            groups.sort_by_key(|(_, members)| best_ping(app, members).unwrap_or(u128::MAX));
        }
        SortMode::Added => {
            groups.sort_by_key(|(_, members)| {
                members.iter().map(|m| m.created_at).max().unwrap_or(0)
            });
            groups.reverse();
        }
    }
    // The implicit "Saved" bucket stays at the bottom like the design.
    if let Some(idx) = groups.iter().position(|(l, _)| l == "Saved") {
        if groups.len() > 1 {
            let saved = groups.remove(idx);
            groups.push(saved);
        }
    }
    groups
}

fn best_ping(app: &App, members: &[Profile]) -> Option<u128> {
    members
        .iter()
        .filter_map(|m| app.pings.get(&m.id).and_then(|p| *p))
        .min()
}
