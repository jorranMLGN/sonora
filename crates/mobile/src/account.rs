//! Signing in to a provider and out of it again, from the phone.
//!
//! A sign-in runs the provider's own `MusicProvider::sign_in` on the core's runtime. Its prompts
//! go out through `on_prompt` and the answers come back through `answer`; Swift answers the ones
//! that need a browser itself (Spotify's authorization page, a cookie sign-in), JS the rest.
//! Spotify's loopback listener on 127.0.0.1 runs inside the app, so the redirect the browser
//! follows lands here; `answer` also forwards a pasted redirect address to it, for a browser
//! that could not reach it.

use std::io::Write as _;
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context as _, Result, anyhow};
use music::{MusicProvider, SignIn, SignInFailure, SignInProblem};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use crate::core::{self, Core};
use crate::player::Command;
use crate::{Answer, CoreError, SessionState};

/// SoundCloud's own sign-in page. The phone runs it in a browser, which the desktop does not,
/// and reads the token out of the cookie the page leaves.
const SOUNDCLOUD: Web = Web {
    url: "https://soundcloud.com/signin",
    landing: "soundcloud.com",
    domain: "soundcloud.com",
    proof: &["oauth_token"],
    cookie: Some("oauth_token"),
};

/// How the listener asked to sign in. `Web` and `Paste` both answer the provider's secret
/// prompt: one in the app's browser, the other with a value the user pastes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SignInOption {
    /// The provider's own account sign-in, such as Spotify's authorization page.
    Account,
    Guest,
    Web,
    Paste,
}

/// A cookie sign-in the app runs in its own browser, as `music::WebSignIn` describes it: `url`
/// opens first, and the user is through once the cookies for `domain` carry one of the `proof`
/// names with a value. The `Cookie` header for `domain` is then the answer.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct WebSignIn {
    pub url: String,
    pub landing: String,
    pub domain: String,
    pub proof: Vec<String>,
    /// A user agent the browser presents instead of its own, when the provider asks for one.
    pub agent: Option<String>,
}

/// Why a provider turned a sign-in down, as `music::SignInProblem` says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Problem {
    Premium,
    Region,
    Credentials,
    Secret,
    Network,
    Cancelled,
    Refused,
}

/// A sign-in under way: where its answers go, how to stop it, and the state to go back to when
/// it is called off.
pub struct Pending {
    attempt: u64,
    input: mpsc::UnboundedSender<String>,
    abort: AbortHandle,
    /// The cookie whose value is the answer, when the provider wants that rather than the
    /// whole header.
    cookie: Option<&'static str>,
    before: SessionState,
}

/// A browser sign-in the phone offers beyond what the provider describes itself.
struct Web {
    url: &'static str,
    landing: &'static str,
    domain: &'static str,
    proof: &'static [&'static str],
    cookie: Option<&'static str>,
}

/// Signs in to the provider `slug` the way `option` names, and waits until that is done. Any
/// prompt on the way goes out through `on_prompt`. A sign-in called off with `cancel_sign_in`
/// ends quietly; one the provider turns down fails with its reason.
#[uniffi::export]
pub async fn sign_in(slug: String, option: SignInOption) -> Answer<()> {
    let core = core::get()?;
    let provider = core
        .providers
        .iter()
        .find(|provider| provider.slug() == slug)
        .cloned()
        .with_context(|| format!("no provider is called {slug}"))?;
    let slug = provider.slug();
    let (method, web) = method(provider.as_ref(), option)?;
    let current = core.sessions.state(slug);
    if current == SessionState::Connecting && !pending(&core, slug) {
        return Err(anyhow!("{slug} is still connecting, try again in a moment").into());
    }
    let before = stop(&core, slug).unwrap_or(current);

    let attempt = ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    let (input, answers) = mpsc::unbounded_channel();
    let prompts = core::prompt_sink(core.events.clone(), slug, web.as_ref().map(Web::record));
    core.sessions.set(slug, SessionState::Connecting);
    core.events.session(slug, SessionState::Connecting);

    let worker = core.clone();
    let task = {
        // Held while the task is spawned and filed, so even a sign-in that ends at once finds
        // itself filed when it looks.
        let mut pending = core.pending.lock().map_err(|_| anyhow!("poisoned"))?;
        let task = core.runtime.spawn(async move {
            let signed = provider.sign_in(method, prompts, answers).await;
            finish(&worker, slug, attempt, signed)
        });
        pending.insert(
            slug,
            Pending {
                attempt,
                input,
                abort: task.abort_handle(),
                cookie: web.and_then(|web| web.cookie),
                before,
            },
        );
        task
    };
    match task.await {
        Ok(outcome) => outcome,
        Err(stopped) if stopped.is_cancelled() => Ok(()),
        Err(stopped) => Err(anyhow!("the sign-in stopped: {stopped}").into()),
    }
}

/// Answers the prompt the sign-in of `slug` is waiting on: a pasted secret, a cookie header or
/// the id of a chosen account. A loopback address, such as the page Spotify's authorization
/// redirects to, is delivered to the listener in the app instead.
#[uniffi::export]
pub fn answer(slug: String, input: String) -> Answer<()> {
    let core = core::get()?;
    if let Some(target) = loopback(&input) {
        core.runtime.spawn_blocking(move || {
            if let Err(error) = deliver(&target) {
                log::warn!("account: cannot hand the redirect to the app: {error:#}");
            }
        });
        return Ok(());
    }
    let pending = core.pending.lock().map_err(|_| anyhow!("poisoned"))?;
    let waiting = pending
        .get(slug.as_str())
        .with_context(|| format!("no sign-in to {slug} is waiting for an answer"))?;
    let input = match waiting.cookie {
        Some(name) => cookie(&input, name).map_or(input.clone(), str::to_owned),
        None => input,
    };
    waiting
        .input
        .send(input)
        .map_err(|_| anyhow!("the sign-in to {slug} has already ended"))?;
    Ok(())
}

/// Calls off the sign-in to `slug`, if one is under way, and puts its session back where it
/// was.
#[uniffi::export]
pub fn cancel_sign_in(slug: String) -> Answer<()> {
    let core = core::get()?;
    let Some(slug) = known(&core, &slug) else {
        return Ok(());
    };
    if let Some(before) = stop(&core, slug) {
        core.sessions.set(slug, before.clone());
        core.events.session(slug, before);
    }
    Ok(())
}

/// Signs out of `slug`: its stored credentials, its live session, its tracks in the queue, its
/// library listings, its saved ids and its snapshots go. SoundCloud then carries on as a guest, as on a fresh install.
#[uniffi::export]
pub fn sign_out(slug: String) -> Answer<()> {
    let core = core::get()?;
    let provider = core
        .providers
        .iter()
        .find(|provider| provider.slug() == slug)
        .cloned()
        .with_context(|| format!("no provider is called {slug}"))?;
    let slug = provider.slug();
    stop(&core, slug);
    provider.sign_out();
    core.sessions.remove(slug);
    core.sessions.set(slug, SessionState::SignedOut);
    core.events.session(slug, SessionState::SignedOut);
    core.send(Command::Forget { slug, tracks: true })?;
    core.library.forget(slug);
    if slug == "soundcloud" {
        core.restore_one(provider);
    }
    Ok(())
}

/// The ways the phone offers to sign in to `provider`. Reading a desktop browser's cookies is
/// not one of them.
pub fn options(provider: &dyn MusicProvider) -> Vec<SignInOption> {
    let web = provider.web_sign_in().is_some() || extra(provider.slug()).is_some();
    provider
        .sign_in_options()
        .into_iter()
        .flat_map(|method| match method {
            SignIn::Default => vec![SignInOption::Account],
            SignIn::Anonymous => vec![SignInOption::Guest],
            SignIn::Secret => match (web, provider.slug()) {
                (true, "soundcloud") => vec![SignInOption::Web, SignInOption::Paste],
                (true, _) => vec![SignInOption::Web],
                (false, _) => vec![SignInOption::Paste],
            },
            SignIn::Browser(_) | SignIn::Path(_) | SignIn::Credentials { .. } => Vec::new(),
        })
        .collect()
}

/// The phone's word for a provider's reason.
pub fn problem(problem: SignInProblem) -> Problem {
    match problem {
        SignInProblem::Premium => Problem::Premium,
        SignInProblem::Region => Problem::Region,
        SignInProblem::Credentials => Problem::Credentials,
        SignInProblem::Secret => Problem::Secret,
        SignInProblem::Network => Problem::Network,
        SignInProblem::Cancelled => Problem::Cancelled,
        SignInProblem::Refused => Problem::Refused,
    }
}

/// Numbers every sign-in, so one that ends after it was replaced knows to keep quiet.
static ATTEMPTS: AtomicU64 = AtomicU64::new(0);

impl Web {
    fn record(&self) -> WebSignIn {
        WebSignIn {
            url: self.url.to_owned(),
            landing: self.landing.to_owned(),
            domain: self.domain.to_owned(),
            proof: self.proof.iter().map(|name| (*name).to_owned()).collect(),
            agent: None,
        }
    }
}

/// The provider's method for `option`, and the browser sign-in that answers its secret prompt.
fn method(provider: &dyn MusicProvider, option: SignInOption) -> Result<(SignIn, Option<Web>)> {
    let offered = options(provider);
    if !offered.contains(&option) {
        anyhow::bail!("{} does not sign in that way", provider.name());
    }
    Ok(match option {
        SignInOption::Account => (SignIn::Default, None),
        SignInOption::Guest => (SignIn::Anonymous, None),
        SignInOption::Paste => (SignIn::Secret, None),
        SignInOption::Web => {
            let web = provider
                .web_sign_in()
                .map(|web| Web {
                    url: web.url,
                    landing: web.landing,
                    domain: web.domain,
                    proof: web.proof,
                    cookie: None,
                })
                .or_else(|| extra(provider.slug()))
                .context("no browser sign-in")?;
            (SignIn::Secret, Some(web))
        }
    })
}

/// A browser sign-in the phone adds for a provider that describes none itself.
fn extra(slug: &str) -> Option<Web> {
    match slug {
        "soundcloud" => Some(SOUNDCLOUD),
        _ => None,
    }
}

/// Takes in what a sign-in ended with, unless it was called off or replaced meanwhile.
fn finish(
    core: &Core,
    slug: &'static str,
    attempt: u64,
    signed: Result<music::ProviderSession>,
) -> Answer<()> {
    let before = {
        let mut pending = core.pending.lock().map_err(|_| anyhow!("poisoned"))?;
        match pending.get(slug) {
            Some(waiting) if waiting.attempt == attempt => pending.remove(slug).map(|p| p.before),
            // Called off or replaced: whoever did that has put the state right.
            Some(_) => return Ok(()),
            // Finished before it was even registered; nothing to go back to but signed out.
            None => None,
        }
    };
    match signed {
        Ok(session) => {
            let state = core.sessions.install(slug, session);
            core.sessions.set(slug, state.clone());
            core.events.session(slug, state);
            core.send(Command::Forget {
                slug,
                tracks: false,
            })?;
            log::info!("account: signed in to {slug}");
            Ok(())
        }
        Err(error) => {
            log::warn!("account: cannot sign in to {slug}: {error:#}");
            let before = match before {
                Some(SessionState::Connecting) | None => match core.sessions.session(slug) {
                    Some(_) => SessionState::Guest,
                    None => SessionState::SignedOut,
                },
                Some(before) => before,
            };
            core.sessions.set(slug, before.clone());
            core.events.session(slug, before);
            Err(match error.downcast_ref::<SignInFailure>() {
                Some(SignInFailure(reason)) => CoreError::SignIn {
                    problem: problem(*reason),
                    message: format!("{error:#}"),
                },
                None => error.into(),
            })
        }
    }
}

/// Stops the sign-in to `slug`, if one is under way, and answers the state it started from.
fn stop(core: &Core, slug: &'static str) -> Option<SessionState> {
    let stopped = core.pending.lock().ok()?.remove(slug)?;
    stopped.abort.abort();
    if let Some(provider) = core
        .providers
        .iter()
        .find(|provider| provider.slug() == slug)
        .cloned()
    {
        // Spotify's listener waits on a blocking thread; abandoning knocks on it so it lets go.
        core.runtime.spawn_blocking(move || provider.abandon());
    }
    log::debug!("account: called off the sign-in to {slug}");
    Some(stopped.before)
}

fn pending(core: &Core, slug: &str) -> bool {
    core.pending
        .lock()
        .is_ok_and(|pending| pending.contains_key(slug))
}

fn known(core: &Core, slug: &str) -> Option<&'static str> {
    core.providers
        .iter()
        .map(|provider| provider.slug())
        .find(|known| *known == slug)
}

/// The `host:port` and request target of an `http` address on this device, such as
/// `http://127.0.0.1:8989/login?code=…`.
fn loopback(input: &str) -> Option<(String, String)> {
    let rest = input.trim().strip_prefix("http://")?;
    let (authority, target) = match rest.find('/') {
        Some(slash) => (&rest[..slash], &rest[slash..]),
        None => (rest, "/"),
    };
    let (host, port) = authority.split_once(':').unwrap_or((authority, "80"));
    if !matches!(host, "127.0.0.1" | "localhost") || !port.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((format!("127.0.0.1:{port}"), target.to_owned()))
}

/// Makes the request a browser would have made to the listener in the app.
fn deliver((address, target): &(String, String)) -> Result<()> {
    let mut stream =
        TcpStream::connect(address).with_context(|| format!("cannot connect to {address}"))?;
    write!(stream, "GET {target} HTTP/1.1\r\nHost: {address}\r\n\r\n")
        .context("cannot send the redirect")?;
    Ok(())
}

/// The value of the cookie `name` in a `Cookie` header.
fn cookie<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, value)| *key == name && !value.trim().is_empty())
        .map(|(_, value)| value)
}
