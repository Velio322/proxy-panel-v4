(function() {
  var btn = document.getElementById('menuToggle');
  var nav = document.getElementById('navMenu');
  if (btn && nav) {
    btn.addEventListener('click', function() { nav.classList.toggle('open'); });
  }
  var search = document.getElementById('tableFilter');
  if (search) {
    search.addEventListener('input', function() {
      var q = search.value.toLowerCase();
      document.querySelectorAll('#dataTable tbody tr').forEach(function(tr) {
        tr.style.display = tr.textContent.toLowerCase().indexOf(q) !== -1 ? '' : 'none';
      });
    });
  }
  document.querySelectorAll('[data-billing]').forEach(function(b) {
    b.addEventListener('click', function() {
      document.querySelectorAll('[data-billing]').forEach(function(x) { x.classList.remove('active'); });
      b.classList.add('active');
      var mode = b.getAttribute('data-billing');
      document.querySelectorAll('[data-price-m]').forEach(function(el) {
        el.textContent = mode === 'annual' ? el.getAttribute('data-price-y') : el.getAttribute('data-price-m');
      });
    });
  });
  var form = document.getElementById('contactForm');
  var formMsg = document.getElementById('formFeedback');
  if (form && formMsg) {
    form.addEventListener('submit', function(e) {
      e.preventDefault();
      formMsg.textContent = 'Thank you. Your inquiry reference #' + Math.floor(100000 + Math.random() * 900000) + ' has been logged.';
      form.reset();
    });
  }
})();
