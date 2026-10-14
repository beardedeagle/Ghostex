//! The `/api/agentAccounts` answer the account menus read, and nothing else of it.
//!
//! CDXC:AgentProviders 2026-09-21 WHY:
//! The account pages read a small part of `AgentAccountsState` (packages/shared/agent-accounts.ts):
//! each account's id, provider, names, registration, status, usage windows and reset credits, the
//! per-provider default account and the session's own provider and account. Those fields are
//! parsed with the JavaScript meaning the TypeScript gives them (a missing optional field is
//! `undefined`, `registered` is read for truthiness, `resetCredits` is kept when it is not
//! `null`), and the same parse is what shapes a remote machine's answer before anything else sees
//! it (`to_json`), so the fields a menu reads are by construction the fields that cross the
//! tunnel. A shape the TypeScript would throw on (no account list, an account without a name or a
//! usage list, no `defaultAccounts`) is an error answer here: the TypeScript either showed an
//! engine error text or published nothing, neither of which can be reproduced.
//!
//! SEE-ALSO: packages/shared/agent-accounts.ts, server/src/accounts/.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

/// What a malformed answer reads as.
pub const INVALID_ACCOUNTS_ANSWER: &str = "gxserver returned an invalid account list.";

/// One usage window (`AccountUsageWindow`).
#[derive(Clone, Debug, PartialEq)]
pub struct AccountUsageWindow {
    pub id: Option<String>,
    pub label: Option<String>,
    pub used_percent: f64,
    pub limit_window_seconds: Option<f64>,
    pub resets_at: Option<String>,
    pub model: Option<String>,
}

impl AccountUsageWindow {
    /// `window.model` as a truthy value: an empty model is no model.
    pub(crate) fn scoped_model(&self) -> Option<&str> {
        self.model.as_deref().filter(|model| !model.is_empty())
    }
}

/// `resetCredits` when it is not `null`: a number or, from a daemon that sends one, a string.
#[derive(Clone, Debug, PartialEq)]
pub enum ResetCredits {
    Number(f64),
    Text(String),
}

/// One account (`AgentAccount`), the fields the menus read.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentAccount {
    pub id: Option<String>,
    pub provider: Option<String>,
    pub selector: Option<String>,
    pub indicator: Option<String>,
    pub name: String,
    pub email: Option<String>,
    pub registered: bool,
    /// The user turned the account off in Settings > Accounts: it stays saved but no menu offers
    /// it (gxserver's `CDXC:AgentProviders 2026-10-14` on `SavedAccount::user_disabled`).
    pub user_disabled: bool,
    pub status: Option<String>,
    pub usage: Vec<AccountUsageWindow>,
    pub reset_credits: Option<ResetCredits>,
}

/// `AgentAccountsState.session`, the fields the menus read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccountSession {
    pub provider: Option<String>,
    pub account_id: Option<String>,
    /// The session runs on its own login, so its flyout offers no account (gxserver's
    /// `CDXC:AgentProviders 2026-10-09` in server/src/accounts/launch.rs).
    pub uses_own_login: bool,
}

/// `AgentAccountsState`, the part the account menus read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccountsState {
    pub accounts: Vec<AgentAccount>,
    /// `defaultAccounts`: provider to account id.
    pub default_accounts: BTreeMap<String, String>,
    pub session: Option<AccountSession>,
}

impl AccountsState {
    /// Reads a daemon answer. `Err` carries the text the page shows.
    pub fn from_json(value: &Value) -> Result<Self, String> {
        let invalid = || INVALID_ACCOUNTS_ANSWER.to_string();
        let object = value.as_object().ok_or_else(invalid)?;
        let accounts = object
            .get("accounts")
            .and_then(Value::as_array)
            .ok_or_else(invalid)?
            .iter()
            .map(account)
            .collect::<Option<Vec<_>>>()
            .ok_or_else(invalid)?;
        let mut default_accounts = BTreeMap::new();
        for (provider, id) in object
            .get("defaultAccounts")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?
        {
            match id {
                Value::String(id) => {
                    default_accounts.insert(provider.clone(), id.clone());
                }
                // `undefined` and `null` compare unequal to every id and launch with none.
                Value::Null => {}
                _ => return Err(invalid()),
            }
        }
        let session = match object.get("session") {
            None | Some(Value::Null) => None,
            Some(Value::Object(session)) => Some(AccountSession {
                provider: optional_text(session.get("provider")).ok_or_else(invalid)?,
                account_id: optional_text(session.get("accountId")).ok_or_else(invalid)?,
                uses_own_login: truthy(session.get("usesOwnLogin")),
            }),
            Some(_) => return Err(invalid()),
        };
        Ok(Self {
            accounts,
            default_accounts,
            session,
        })
    }

    /// The answer as the menus read it, which is also all of it a remote machine may send on.
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "accounts".to_string(),
            Value::Array(self.accounts.iter().map(account_json).collect()),
        );
        object.insert(
            "defaultAccounts".to_string(),
            Value::Object(
                self.default_accounts
                    .iter()
                    .map(|(provider, id)| (provider.clone(), Value::String(id.clone())))
                    .collect(),
            ),
        );
        if let Some(session) = &self.session {
            let mut entry = Map::new();
            put_text(&mut entry, "provider", &session.provider);
            entry.insert(
                "accountId".to_string(),
                session
                    .account_id
                    .as_ref()
                    .map_or(Value::Null, |id| Value::String(id.clone())),
            );
            if session.uses_own_login {
                entry.insert("usesOwnLogin".to_string(), Value::Bool(true));
            }
            object.insert("session".to_string(), Value::Object(entry));
        }
        Value::Object(object)
    }

    /// `accounts.filter((account) => account.registered && account.provider === provider)`, where
    /// `undefined === undefined` holds, as it does in the session page's filter, leaving out the
    /// accounts the user turned off.
    pub(crate) fn registered_for<'a>(
        &'a self,
        provider: Option<&'a str>,
    ) -> impl Iterator<Item = &'a AgentAccount> + 'a {
        self.accounts.iter().filter(move |account| {
            account.registered && !account.user_disabled && account.provider.as_deref() == provider
        })
    }

    /// `data.session?.accountId`.
    pub(crate) fn session_account_id(&self) -> Option<&str> {
        self.session
            .as_ref()
            .and_then(|session| session.account_id.as_deref())
    }

    /// `quickLaunchAccountId(state, provider)`: the account the provider's rule picked for new
    /// sessions, resolved by gxserver (`CDXC:AgentProviders 2026-09-11 DECISION`).
    pub fn quick_launch_account_id(&self, provider: &str) -> Option<&str> {
        self.default_accounts.get(provider).map(String::as_str)
    }
}

/// A string, `undefined`/`null` as `None`, anything else a malformed answer.
fn optional_text(value: Option<&Value>) -> Option<Option<String>> {
    match value {
        None | Some(Value::Null) => Some(None),
        Some(Value::String(text)) => Some(Some(text.clone())),
        Some(_) => None,
    }
}

fn optional_number(value: Option<&Value>) -> Option<Option<f64>> {
    match value {
        None | Some(Value::Null) => Some(None),
        Some(Value::Number(number)) => number.as_f64().map(Some),
        Some(_) => None,
    }
}

/// JavaScript truthiness of a JSON value.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

fn account(value: &Value) -> Option<AgentAccount> {
    let object = value.as_object()?;
    let name = object.get("name")?.as_str()?.to_string();
    let usage = object
        .get("usage")?
        .as_array()?
        .iter()
        .map(window)
        .collect::<Option<Vec<_>>>()?;
    let reset_credits = match object.get("resetCredits") {
        None | Some(Value::Null) => None,
        Some(Value::Number(number)) => Some(ResetCredits::Number(number.as_f64()?)),
        Some(Value::String(text)) => Some(ResetCredits::Text(text.clone())),
        Some(_) => return None,
    };
    Some(AgentAccount {
        id: optional_text(object.get("id"))?,
        provider: optional_text(object.get("provider"))?,
        selector: optional_text(object.get("selector"))?,
        indicator: optional_text(object.get("indicator"))?,
        name,
        email: optional_text(object.get("email"))?,
        registered: truthy(object.get("registered")),
        user_disabled: truthy(object.get("userDisabled")),
        status: optional_text(object.get("status"))?,
        usage,
        reset_credits,
    })
}

fn window(value: &Value) -> Option<AccountUsageWindow> {
    let object = value.as_object()?;
    Some(AccountUsageWindow {
        id: optional_text(object.get("id"))?,
        label: optional_text(object.get("label"))?,
        used_percent: object.get("usedPercent")?.as_f64()?,
        limit_window_seconds: optional_number(object.get("limitWindowSeconds"))?,
        resets_at: optional_text(object.get("resetsAt"))?,
        model: optional_text(object.get("model"))?,
    })
}

fn put_text(object: &mut Map<String, Value>, key: &str, value: &Option<String>) {
    if let Some(value) = value {
        object.insert(key.to_string(), Value::String(value.clone()));
    }
}

fn account_json(account: &AgentAccount) -> Value {
    let mut object = Map::new();
    put_text(&mut object, "id", &account.id);
    put_text(&mut object, "provider", &account.provider);
    put_text(&mut object, "selector", &account.selector);
    put_text(&mut object, "indicator", &account.indicator);
    object.insert("name".to_string(), Value::String(account.name.clone()));
    put_text(&mut object, "email", &account.email);
    object.insert("registered".to_string(), Value::Bool(account.registered));
    if account.user_disabled {
        object.insert("userDisabled".to_string(), Value::Bool(true));
    }
    put_text(&mut object, "status", &account.status);
    object.insert(
        "usage".to_string(),
        Value::Array(
            account
                .usage
                .iter()
                .map(|window| {
                    let mut entry = Map::new();
                    put_text(&mut entry, "id", &window.id);
                    put_text(&mut entry, "label", &window.label);
                    entry.insert(
                        "usedPercent".to_string(),
                        serde_json::Number::from_f64(window.used_percent)
                            .map_or(Value::Null, Value::Number),
                    );
                    if let Some(seconds) = window.limit_window_seconds {
                        entry.insert(
                            "limitWindowSeconds".to_string(),
                            serde_json::Number::from_f64(seconds)
                                .map_or(Value::Null, Value::Number),
                        );
                    }
                    put_text(&mut entry, "resetsAt", &window.resets_at);
                    put_text(&mut entry, "model", &window.model);
                    Value::Object(entry)
                })
                .collect(),
        ),
    );
    match &account.reset_credits {
        Some(ResetCredits::Number(number)) => {
            object.insert(
                "resetCredits".to_string(),
                serde_json::Number::from_f64(*number).map_or(Value::Null, Value::Number),
            );
        }
        Some(ResetCredits::Text(text)) => {
            object.insert("resetCredits".to_string(), Value::String(text.clone()));
        }
        None => {}
    }
    Value::Object(object)
}
