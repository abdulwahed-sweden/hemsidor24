// Order form behaviour. Mirrors what the original page did on submit:
// block the default submit, focus the first invalid field, otherwise show the
// confirmation and lock the button.
//
// Phase 3 replaces this with a real POST to /bestall. Until then the form does
// not reach the server, exactly as it did not before.
document.addEventListener('DOMContentLoaded', function () {
  var form = document.querySelector('form[data-m="form"]');
  if (!form) return;

  form.addEventListener('submit', function (event) {
    event.preventDefault();

    var invalid = form.querySelector(':invalid');
    if (invalid) {
      invalid.focus();
      return;
    }

    var status = form.querySelector('p[role="status"]');
    if (status) status.hidden = false;

    var button = form.querySelector('button[type="submit"]');
    if (button) {
      button.disabled = true;
      button.style.opacity = '0.55';
    }
  });
});

// "Välj Start" / "Välj Pro" preselect the package they name.
//
// The links have always jumped to the order form; before this they left the
// package radio on whatever the server rendered, so choosing Pro from the
// price table landed you on a form still set to Start. Matching on the label's
// leading word rather than its full value keeps this working when the price in
// the copy changes.
//
// Progressive enhancement: without scripting the links still jump, exactly as
// they do today.
document.addEventListener('DOMContentLoaded', function () {
  var pickers = document.querySelectorAll('[data-pick]');

  Array.prototype.forEach.call(pickers, function (picker) {
    picker.addEventListener('click', function () {
      var wanted = picker.getAttribute('data-pick');
      var radios = document.querySelectorAll('input[name="paket"]');

      Array.prototype.forEach.call(radios, function (radio) {
        if (radio.value.indexOf(wanted) === 0) {
          radio.checked = true;
          // Let anything listening on the group know, the same as a real click.
          radio.dispatchEvent(new Event('change', { bubbles: true }));
        }
      });
    });
  });
});
