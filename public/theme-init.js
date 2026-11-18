// Runs synchronously before first paint. Keep in sync with src/lib/theme.tsx.
(function () {
  var pref = "system";
  try {
    pref = localStorage.getItem("accountsvu.theme") || "system";
  } catch (e) {}
  var dark =
    pref === "dark" ||
    (pref === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
})();
