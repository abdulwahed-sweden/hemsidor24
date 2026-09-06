// The order form intentionally uses the browser's native POST to /bestall.
// Do not intercept its submit event here: the server stores the order first and
// only then renders the success state.

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
