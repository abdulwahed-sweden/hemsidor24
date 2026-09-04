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
    fn no_line_is_too_wide_to_read_on_a_phone() {
        let order = order();
        for m in [
            studio_notification(&order, 7, "studio@example.se", "no-reply@example.se"),
            customer_confirmation(&order, "studio@example.se", "no-reply@example.se"),
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
