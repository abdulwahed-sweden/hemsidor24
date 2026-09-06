//! The order form's markup must carry its own submission.
//!
//! `static_js.rs` guards the other half of this: that no script intercepts the
//! submit. Both halves are needed, and neither implies the other. A script that
//! keeps its hands off a form with no `action` still sends nothing, and a
//! correct `action` is no protection against a listener that cancels it.
//!
//! The bug that prompted both: a pre-Phase-3 script cancelled the submit and
//! stood in for the server, and it survived Phase 3 building the real route.
//! Every browser click was swallowed while the suite stayed green, because the
//! tests posted to `/bestall` directly and never loaded the page.

const FORM: &str = include_str!("../templates/partials/order_form.html");

#[test]
fn the_form_posts_itself_to_the_real_endpoint() {
    assert!(
        FORM.contains(r#"method="post""#),
        "the order form must submit itself — that is the only way an order \
         reaches Postgres"
    );
    assert!(
        FORM.contains(r#"action="/bestall""#),
        "the order form must post to /bestall"
    );
}

#[test]
fn the_confirmation_is_the_servers_to_give() {
    // The customer must never read "Tack" before the row is written. The
    // message therefore sits behind a server-side conditional; the old script
    // tried to reveal it client-side, which would have told a customer their
    // order was safe when nothing had been stored.
    let sent_block = FORM
        .split_once("{% if sent %}")
        .expect("the confirmation must be rendered only when the server set sent")
        .1;
    let sent_block = sent_block
        .split_once("{% endif %}")
        .expect("the confirmation block must be closed")
        .0;

    assert!(
        sent_block.contains("Tack."),
        "the confirmation text belongs inside the sent block, not outside it"
    );
}

/// The shipped routing source, checked the same way the markup is.
///
/// A unit test would need a request through the router, and that needs a tower
/// `ServiceExt` this crate does not depend on. Reading the source is cruder but
/// it guards the thing that actually broke, and adds no dependency for it.
const ROUTES: &str = include_str!("../src/routes.rs");

#[test]
fn static_assets_are_revalidated_rather_than_assumed_fresh() {
    // ServeDir sends only `last-modified`, and a browser given only that will
    // cache heuristically and reuse the file without asking. A fixed main.js
    // then does nothing for anyone still holding the broken one — which is how
    // a repaired order form goes on swallowing clicks.
    assert!(
        ROUTES.contains("CACHE_CONTROL"),
        "the static service must set Cache-Control, or a browser may keep \
         serving an old main.js after the fix has shipped"
    );
    assert!(
        ROUTES.contains("no-cache"),
        "static assets must revalidate; `no-cache` still answers 304 when the \
         file has not changed, so it costs almost nothing"
    );
}
