import SafariServices
import UIKit
import WebKit

/// Safari's own user agent on the iPhone. WebKit's default leaves out the `Version/… Safari/…`
/// tail, and Google refuses to sign in a browser it reads as embedded. The desktop's macOS
/// window presents the Mac version of the same string for the same reason.
let safariAgent =
  "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1"

/// Whether a cookie stored for `stored` is sent to `domain` and its subdomains, as the desktop's
/// `webview` crate decides it.
func cookieMatches(_ stored: String, _ domain: String) -> Bool {
  let stored = stored.hasPrefix(".") ? String(stored.dropFirst()) : stored
  return stored == domain || stored.hasSuffix("." + domain)
}

/// The controller everything else is presented over.
@MainActor
func topController() -> UIViewController? {
  let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
  let windows = scenes.flatMap { $0.windows }
  var top = (windows.first { $0.isKeyWindow } ?? windows.first)?.rootViewController
  while let next = top?.presentedViewController {
    top = next
  }
  return top
}

/// Answers the sign-in prompts that need a browser. A `Url` prompt (Spotify's authorization)
/// opens in an in-app Safari: librespot's listener on 127.0.0.1 runs in this process and takes
/// the redirect, so nothing comes back through here. A `Secret` prompt with a web sign-in opens a
/// `WKWebView` over a throwaway data store and sends the cookie header back once the proof
/// cookies are there. Both close when the provider's session leaves `Connecting`; closing one by
/// hand calls the sign-in off.
@MainActor
final class SignInPresenter: NSObject, SFSafariViewControllerDelegate {
  static let shared = SignInPresenter()

  /// What is on the screen for each provider's sign-in.
  private var open: [String: UIViewController] = [:]

  /// Takes every prompt the core sends; the ones without a browser are left to JS.
  func prompt(_ slug: String, _ prompt: Prompt) {
    switch prompt {
    case .url(let url):
      browse(slug, url)
    case .secret(let web?):
      page(slug, web)
    default:
      break
    }
  }

  /// Closes whatever is open for `slug` once its sign-in has an outcome.
  func session(_ slug: String, _ state: SessionState) {
    if case .connecting = state { return }
    close(slug)
  }

  /// Closes the browser of `slug` without calling the sign-in off.
  func close(_ slug: String) {
    guard let controller = open.removeValue(forKey: slug) else { return }
    let navigation = controller as? UINavigationController
    if let page = navigation?.viewControllers.first as? WebSignInController {
      page.stop()
    }
    controller.presentingViewController?.dismiss(animated: true)
  }

  /// The user closed the browser of `slug` by hand.
  func abandoned(_ slug: String) {
    guard open.removeValue(forKey: slug) != nil else { return }
    do {
      try SonoraCore.cancelSignIn(slug: slug)
    } catch {
      NSLog("sonora: cannot call the sign-in off: \(Bridge.message(error))")
    }
  }

  private func browse(_ slug: String, _ address: String) {
    guard let url = URL(string: address), url.scheme == "https" || url.scheme == "http" else {
      NSLog("sonora: the sign-in url does not open in a browser: \(address)")
      return
    }
    close(slug)
    let safari = SFSafariViewController(url: url)
    safari.dismissButtonStyle = .cancel
    safari.delegate = self
    safari.isModalInPresentation = true
    open[slug] = safari
    topController()?.present(safari, animated: true)
  }

  private func page(_ slug: String, _ web: WebSignIn) {
    close(slug)
    let page = WebSignInController(slug: slug, web: web)
    let navigation = UINavigationController(rootViewController: page)
    navigation.isModalInPresentation = true
    open[slug] = navigation
    topController()?.present(navigation, animated: true)
  }

  /// The slug whose browser `controller` is.
  private func slug(of controller: ObjectIdentifier) -> String? {
    open.first { ObjectIdentifier($0.value) == controller }?.key
  }

  nonisolated func safariViewControllerDidFinish(_ controller: SFSafariViewController) {
    let id = ObjectIdentifier(controller)
    DispatchQueue.main.async {
      let presenter = SignInPresenter.shared
      if let slug = presenter.slug(of: id) {
        presenter.abandoned(slug)
      }
    }
  }
}

/// A provider's cookie sign-in in a `WKWebView` over a non-persistent data store, the same
/// contract as the desktop's `webview` crate: once the cookies for `domain` carry one of the
/// `proof` names with a value, their `Cookie` header is the answer.
final class WebSignInController: UIViewController {
  /// How often the cookies are read.
  private static let poll: UInt64 = 500_000_000

  private let slug: String
  private let web: WebSignIn
  private var webView: WKWebView?
  private var polling: Task<Void, Never>?
  /// Whether the sign-in url was loaded a second time; once is the limit.
  private var retried = false

  init(slug: String, web: WebSignIn) {
    self.slug = slug
    self.web = web
    super.init(nibName: nil, bundle: nil)
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("not built from a storyboard")
  }

  override func viewDidLoad() {
    super.viewDidLoad()
    let configuration = WKWebViewConfiguration()
    configuration.websiteDataStore = .nonPersistent()
    let page = WKWebView(frame: view.bounds, configuration: configuration)
    page.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    page.customUserAgent = web.agent ?? safariAgent
    view.addSubview(page)
    webView = page
    title = web.landing
    navigationItem.leftBarButtonItem = UIBarButtonItem(
      barButtonSystemItem: .cancel, target: self, action: #selector(cancel))
    if let url = URL(string: web.url) {
      page.load(URLRequest(url: url))
    }
    polling = Task { @MainActor [weak self] in
      while !Task.isCancelled {
        try? await Task.sleep(nanoseconds: WebSignInController.poll)
        if Task.isCancelled { return }
        await self?.check()
      }
    }
  }

  /// Stops reading cookies; the controller is on its way out.
  func stop() {
    polling?.cancel()
    polling = nil
  }

  @objc private func cancel() {
    stop()
    SignInPresenter.shared.abandoned(slug)
    dismiss(animated: true)
  }

  private func check() async {
    guard let page = webView else { return }
    let cookies = await page.configuration.websiteDataStore.httpCookieStore.allCookies()
    let sent = cookies.filter { cookieMatches($0.domain, web.domain) }
    let proven = sent.contains {
      web.proof.contains($0.name) && !$0.value.trimmingCharacters(in: .whitespaces).isEmpty
    }
    guard proven else {
      retry(page)
      return
    }
    stop()
    let header = sent.map { "\($0.name)=\($0.value)" }.joined(separator: "; ")
    do {
      try SonoraCore.answer(slug: slug, input: header)
    } catch {
      NSLog("sonora: cannot hand the cookies over: \(Bridge.message(error))")
    }
    SignInPresenter.shared.close(slug)
  }

  /// Loads the sign-in url once more when the page landed on the provider's domain signed out:
  /// an account page can skip the hop that hands the account to the provider. A sign-in that
  /// starts on the provider's own domain has no such hop.
  private func retry(_ page: WKWebView) {
    guard !retried, let start = URL(string: web.url), let host = page.url?.host else { return }
    guard !cookieMatches(start.host ?? "", web.domain), cookieMatches(host, web.domain) else {
      return
    }
    retried = true
    page.load(URLRequest(url: start))
  }
}
