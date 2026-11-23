/* Minimal webpack 4/5 + loadable chunk push (meduza.io pattern). */
self.__LOADABLE_LOADED_CHUNKS__ = self.__LOADABLE_LOADED_CHUNKS__ || [];
(function () {
  var s = document.createElement('script');
  s.charset = 'utf-8';
  s.async = true;
  s.src = 'manifest.js';
  s.setAttribute('data-webpack', 'fixture:app');
  document.head.appendChild(s);
  self.__LOADABLE_LOADED_CHUNKS__.push([
    [860],
    {
      860: function () {
        var Te =
          typeof window !== 'undefined' && typeof document !== 'undefined';
        var Pe = (Te && window.location.href) || 'http://localhost';
        var r = {};
        Object.defineProperty(r, 'origin', {
          enumerable: true,
          get: function () {
            return Pe;
          },
        });
        Object.defineProperty(r, 'hasStandardBrowserEnv', {
          enumerable: true,
          get: function () {
            return Te;
          },
        });
        var Me = Object.assign({}, r, { isBrowser: true });
        try {
          var base = new URL(Me.origin);
          var rel = new URL('/api/ping', Me.origin);
          document.getElementById('t').textContent =
            'ok:' + base.origin + ':' + rel.pathname;
        } catch (e) {
          document.getElementById('t').textContent =
            'err:' + e.message + ':' + String(Me.origin);
        }
      },
    },
  ]);
})();