import UIKit
import WebKit

/// Mints YouTube's proof-of-origin tokens in a hidden `WKWebView`, as the desktop's
/// `state::potoken` does in a hidden window. The core hands out what to mint through
/// `poTokenWanted()`: the page, and the desktop's `potoken.js` compiled in from the repo, which
/// runs at document start, attests the engine and leaves the token in a cookie. Whatever comes
/// of it goes back through `poToken`, and the YouTube client uses a cold start token until then.
@MainActor
final class PoTokenMinter {
  static let shared = PoTokenMinter()

  /// How often the core is asked for work and the open page's cookies are read.
  private static let tick: UInt64 = 500_000_000
  /// How long a page gets to attest and mint before it is given up on, as on the desktop.
  private static let patience: TimeInterval = 45

  private var loop: Task<Void, Never>?
  private var page: WKWebView?
  private var ask: PoTokenAsk?
  private var opened = Date()

  /// Starts asking the core for work, for the life of the process. Calling it again does nothing.
  func start() {
    guard loop == nil else { return }
    loop = Task { @MainActor [weak self] in
      while !Task.isCancelled {
        try? await Task.sleep(nanoseconds: PoTokenMinter.tick)
        await self?.step()
      }
    }
  }

  private func step() async {
    guard let ask else {
      begin()
      return
    }
    await carry(ask)
  }

  /// Opens a page for whatever the YouTube client is waiting on, if anything.
  private func begin() {
    guard let ask = SonoraCore.poTokenWanted() else { return }
    guard let url = URL(string: ask.url), let window = window() else {
      SonoraCore.poToken(binding: ask.binding, token: nil)
      return
    }
    let configuration = WKWebViewConfiguration()
    configuration.websiteDataStore = .nonPersistent()
    // A youtube.com page must never start a video, which would take the audio session.
    configuration.mediaTypesRequiringUserActionForPlayback = .all
    configuration.allowsInlineMediaPlayback = false
    configuration.userContentController.addUserScript(
      WKUserScript(source: ask.script, injectionTime: .atDocumentStart, forMainFrameOnly: true))
    // Off the screen but in the window, so WebKit keeps running its scripts.
    let bounds = window.bounds
    let view = WKWebView(
      frame: CGRect(x: -bounds.width * 2, y: 0, width: bounds.width, height: bounds.height),
      configuration: configuration)
    view.customUserAgent = safariAgent
    view.isUserInteractionEnabled = false
    view.accessibilityElementsHidden = true
    window.addSubview(view)
    view.load(URLRequest(url: url))
    page = view
    self.ask = ask
    opened = Date()
  }

  /// Looks for the answer in the page's cookies, and gives up on a page that took too long.
  private func carry(_ ask: PoTokenAsk) async {
    guard let page else {
      done(ask, nil)
      return
    }
    let cookies = await page.configuration.websiteDataStore.httpCookieStore.allCookies()
    let answer = cookies.first {
      $0.name == ask.cookie && cookieMatches($0.domain, ask.domain) && !$0.value.isEmpty
    }
    if let answer {
      done(ask, answer.value)
    } else if Date().timeIntervalSince(opened) > PoTokenMinter.patience {
      NSLog("sonora: the minting page did not answer in time")
      done(ask, nil)
    }
  }

  /// Files the outcome and closes the page. A failure is filed too, which is what holds the
  /// next attempt off rather than opening a page every tick.
  private func done(_ ask: PoTokenAsk, _ token: String?) {
    page?.stopLoading()
    page?.removeFromSuperview()
    page = nil
    self.ask = nil
    SonoraCore.poToken(binding: ask.binding, token: token)
  }

  private func window() -> UIWindow? {
    let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
    let windows = scenes.flatMap { $0.windows }
    return windows.first { $0.isKeyWindow } ?? windows.first
  }
}
