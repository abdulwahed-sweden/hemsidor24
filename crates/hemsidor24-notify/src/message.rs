//! The two messages sent when an order arrives.
//!
//! Both are plain text on purpose. The studio one is read on a phone between
//! other jobs; the customer one is a receipt, not a campaign.

use hemsidor24_core::Order;

/// An email, ready to hand to a [`crate::Notifier`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Envelope recipient.
    pub to: String,
    /// Envelope sender.
    pub from: String,
    /// Where a reply should go, if anywhere in particular.
    pub reply_to: Option<String>,
    /// Subject line.
    pub subject: String,
    /// Plain-text body.
    pub body: String,
}

/// The message telling the studio that work has arrived.
///
/// `order_id` is the row in `orders`, so the admin panel and the inbox agree
/// about which order is which.
pub fn studio_notification(order: &Order, order_id: i64, to: &str, from: &str) -> Message {
    let services = order
        .services()
        .as_slice()
        .iter()
        .map(|s| format!("  - {s}"))
        .collect::<Vec<_>>()
        .join("\n");

    let body = format!(
        "Ny beställning #{order_id}\n\
         \n\
         Företag:  {company}\n\
         Ort:      {city}\n\
         Telefon:  {phone}\n\
         E-post:   {email}\n\
         \n\
         Paket:    {package} ({price} kr exkl. moms)\n\
         Stil:     {style}\n\
         \n\
         Tjänster:\n\
         {services}\n",
        order_id = order_id,
        company = order.company(),
        city = order.city(),
        phone = order.phone(),
        email = order.email(),
        package = order.package().label_sv(),
        price = order.package().price_sek_ex_vat(),
        style = order.style().label_sv(),
        services = services,
    );

    Message {
        to: to.to_owned(),
        from: from.to_owned(),
        // Replying to the notification should reach the customer, not us.
        reply_to: Some(order.email().to_owned()),
        subject: format!(
            "Ny beställning — {} ({})",
            order.company(),
            order.package().label_sv()
        ),
        body,
    }
}

/// The receipt sent to the customer.
pub fn customer_confirmation(order: &Order, to_studio: &str, from: &str) -> Message {
    let body = format!(
        "Hej {company},\n\
         \n\
         Tack för din beställning. Vi har tagit emot den och hör av oss med ett\n\
         förslag inom ett dygn.\n\
         \n\
         Du betalar först när du sett sidan.\n\
         \n\
         Detta valde du:\n\
         \n\
         Paket:    {package} ({price} kr exkl. moms)\n\
         Stil:     {style}\n\
         \n\
         Har du frågor, svara på det här mejlet.\n\
         \n\
         Hemsidor24\n\
         {studio}\n",
        company = order.company(),
        package = order.package().label_sv(),
        price = order.package().price_sek_ex_vat(),
        style = order.style().label_sv(),
        studio = to_studio,
    );

    Message {
        to: order.email().to_owned(),
        from: from.to_owned(),
        reply_to: Some(to_studio.to_owned()),
        subject: "Tack för din beställning — Hemsidor24".to_owned(),
        body,
    }
}

/// The message telling the customer their proposal is ready to look at.
///
/// Sent when the studio moves an order to "Utkast skickat". Until this existed
/// the customer heard nothing between ordering and the site appearing, which
/// left the one promise the product is built on — a proposal within a day,
/// before any money changes hands — depending on somebody remembering to write
/// an email by hand.
///
/// Takes the company and package rather than a whole [`Order`] because the
/// back office reads its rows as plain columns, and reconstructing a validated
/// order just to address an envelope would be work for its own sake. The
/// package is the typed one so the price cannot drift from what was quoted.
pub fn proposal_ready(
    company: &str,
    package: hemsidor24_core::Package,
    to_customer: &str,
    from: &str,
    studio: &str,
) -> Message {
    let body = format!(
        "Hej {company},\n\
         \n\
         Ditt förslag är klart. Vi hör av oss med en länk till sidan, så att du\n\
         kan se den innan den publiceras.\n\
         \n\
         Du betalar först när du sett sidan och sagt ja. Vill du inte ha den\n\
         kostar det ingenting.\n\
         \n\
         Detta gäller din beställning:\n\
         \n\
         Paket:    {package} ({price} kr exkl. moms)\n\
         \n\
         Svara på det här mejlet om något ser fel ut.\n\
         \n\
         Hemsidor24\n\
         {studio}\n",
        company = company,
        package = package.label_sv(),
        price = package.price_sek_ex_vat(),
        studio = studio,
    );

    Message {
        to: to_customer.to_owned(),
        from: from.to_owned(),
        reply_to: Some(studio.to_owned()),
        subject: "Ditt förslag är klart — Hemsidor24".to_owned(),
        body,
    }
}

/// The message confirming the studio has registered the customer's approval.
///
/// Sent when an order moves to "Godkänd". It states plainly what was recorded
/// and what happens next, and it deliberately invents no commercial terms —
/// invoicing, payment periods and the rest are settled by the agreement, not
/// by this envelope.
pub fn proposal_accepted(
    company: &str,
    package: hemsidor24_core::Package,
    to_customer: &str,
    from: &str,
    studio: &str,
) -> Message {
    let body = format!(
        "Hej {company},\n\
         \n\
         Tack. Vi har registrerat att du godkänt förslaget.\n\
         \n\
         Nu publicerar vi sidan. Domän, webbhotell och källkod ska stå i ditt\n\
         namn, och vi hör av oss när allt är på plats.\n\
         \n\
         Detta gäller din beställning:\n\
         \n\
         Paket:    {package} ({price} kr exkl. moms)\n\
         \n\
         Svara på det här mejlet om något ser fel ut.\n\
         \n\
         Hemsidor24\n\
         {studio}\n",
        company = company,
        package = package.label_sv(),
        price = package.price_sek_ex_vat(),
        studio = studio,
    );

    Message {
        to: to_customer.to_owned(),
        from: from.to_owned(),
        reply_to: Some(studio.to_owned()),
        subject: "Vi har registrerat ditt godkännande — Hemsidor24".to_owned(),
        body,
    }
}

/// The message telling the customer their site is live.
///
/// The last thing the studio says in the ordinary course of a job. It names
/// where the site is and what has been put in the customer's name, and it says
/// what is theirs to keep — which is the promise the whole product is sold on.
pub fn site_published(
    company: &str,
    live_url: &str,
    to_customer: &str,
    from: &str,
    studio: &str,
) -> Message {
    let body = format!(
        "Hej {company},\n\
         \n\
         Din sida är publicerad:\n\
         \n\
         {live_url}\n\
         \n\
         Domän, webbhotell och källkod står i ditt namn. Du är inte bunden till\n\
         oss — du kan när som helst låta någon annan arbeta vidare på sidan.\n\
         \n\
         Hör av dig om något behöver rättas.\n\
         \n\
         Hemsidor24\n\
         {studio}\n",
        company = company,
        live_url = live_url,
        studio = studio,
    );

    Message {
        to: to_customer.to_owned(),
        from: from.to_owned(),
        reply_to: Some(studio.to_owned()),
        subject: "Din sida är publicerad — Hemsidor24".to_owned(),
        body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hemsidor24_core::OrderForm;

    fn order() -> Order {
        OrderForm {
            company: "Malmö Bygg AB".into(),
            city: "Malmö".into(),
            phone: "070-123 45 67".into(),
            email: "kontakt@malmobygg.se".into(),
            services: "Badrum 15 000 kr\nKök 25 000 kr".into(),
            style: "Djärv".into(),
            package: "Pro — 4 490 kr".into(),
        }
        .validate()
        .expect("fixture should be valid")
    }

    #[test]
    fn studio_subject_matches_the_agreed_format() {
        let m = studio_notification(&order(), 7, "studio@example.se", "no-reply@example.se");
        assert_eq!(m.subject, "Ny beställning — Malmö Bygg AB (Pro)");
    }

    #[test]
    fn studio_body_carries_every_field() {
        let m = studio_notification(&order(), 7, "studio@example.se", "no-reply@example.se");
        for expected in [
            "Ny beställning #7",
            "Malmö Bygg AB",
            "Malmö",
            "070-123 45 67",
            "kontakt@malmobygg.se",
            "Pro (4490 kr exkl. moms)",
            "Djärv",
            "  - Badrum 15 000 kr",
            "  - Kök 25 000 kr",
        ] {
            assert!(
                m.body.contains(expected),
                "body is missing {expected:?}:\n{}",
                m.body
            );
        }
    }

    #[test]
    fn replying_to_the_studio_mail_reaches_the_customer() {
        let m = studio_notification(&order(), 7, "studio@example.se", "no-reply@example.se");
        assert_eq!(m.to, "studio@example.se");
        assert_eq!(m.reply_to.as_deref(), Some("kontakt@malmobygg.se"));
    }

    #[test]
    fn the_customer_receipt_goes_to_the_customer() {
        let m = customer_confirmation(&order(), "studio@example.se", "no-reply@example.se");
        assert_eq!(m.to, "kontakt@malmobygg.se");
        assert_eq!(m.reply_to.as_deref(), Some("studio@example.se"));
        assert!(m.body.contains("Malmö Bygg AB"));
        assert!(m.body.contains("Pro"));
    }

    #[test]
    fn the_proposal_mail_goes_to_the_customer_and_quotes_their_package() {
        let m = proposal_ready(
            "Malmö Bygg AB",
            hemsidor24_core::Package::Pro,
            "kontakt@malmobygg.se",
            "no-reply@example.se",
            "studio@example.se",
        );
        assert_eq!(m.to, "kontakt@malmobygg.se");
        assert_eq!(m.reply_to.as_deref(), Some("studio@example.se"));
        assert_eq!(m.subject, "Ditt förslag är klart — Hemsidor24");
        assert!(m.body.contains("Malmö Bygg AB"));
        assert!(m.body.contains("Pro (4490 kr exkl. moms)"));
    }

    #[test]
    fn the_proposal_mail_repeats_the_promise_it_settles() {
        let m = proposal_ready(
            "X AB",
            hemsidor24_core::Package::Start,
            "a@b.se",
            "no-reply@example.se",
            "studio@example.se",
        );
        // The guarantee is the reason this mail exists; it must not go out
        // implying the customer already owes something.
        assert!(m.body.contains("Du betalar först när du sett sidan"));
        assert!(m.body.contains("kostar det ingenting"));
        assert!(m.body.contains("2490 kr exkl. moms"));
    }

    #[test]
    fn the_acceptance_mail_records_what_was_agreed_without_inventing_terms() {
        let m = proposal_accepted(
            "Malmö Bygg AB",
            hemsidor24_core::Package::Pro,
            "kontakt@malmobygg.se",
            "no-reply@example.se",
            "studio@example.se",
        );
        assert_eq!(m.to, "kontakt@malmobygg.se");
        assert_eq!(
            m.subject,
            "Vi har registrerat ditt godkännande — Hemsidor24"
        );
        assert!(m.body.contains("godkänt förslaget"));
        assert!(m.body.contains("Pro (4490 kr exkl. moms)"));
        // Payment periods, invoice terms and deadlines belong to the
        // agreement. An envelope must not quietly introduce them.
        for invented in [
            "dagar",
            "faktura",
            "förfaller",
            "ränta",
            "betalningsvillkor",
        ] {
            assert!(
                !m.body.to_lowercase().contains(invented),
                "invented term: {invented}"
            );
        }
    }

    #[test]
    fn the_publication_mail_names_the_site_and_what_is_theirs() {
        let m = site_published(
            "Malmö Bygg AB",
            "https://malmobygg.se",
            "kontakt@malmobygg.se",
            "no-reply@example.se",
            "studio@example.se",
        );
        assert_eq!(m.subject, "Din sida är publicerad — Hemsidor24");
        assert!(m.body.contains("https://malmobygg.se"));
        assert!(m.body.contains("i ditt namn"));
        assert!(m.body.contains("inte bunden till"));
    }

    #[test]
    fn no_line_is_too_wide_to_read_on_a_phone() {
        let order = order();
        for m in [
            studio_notification(&order, 7, "studio@example.se", "no-reply@example.se"),
            customer_confirmation(&order, "studio@example.se", "no-reply@example.se"),
            proposal_ready(
                "Malmö Bygg AB",
                hemsidor24_core::Package::Pro,
                "kontakt@malmobygg.se",
                "no-reply@example.se",
                "studio@example.se",
            ),
        ] {
            for line in m.body.lines() {
                assert!(
                    line.chars().count() <= 72,
                    "line too long ({}): {line:?}",
                    line.chars().count()
                );
            }
        }
    }
}
