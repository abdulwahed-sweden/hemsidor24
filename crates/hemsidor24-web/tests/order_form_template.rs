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
