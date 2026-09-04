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
