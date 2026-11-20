(function () {
  globalThis.__SPIKE_HARNESS__ = { status: "OK", tests: [] };
  globalThis.__SPIKE_TESTS__ = globalThis.__SPIKE_HARNESS__.tests;

  add_result_callback(function (test) {
    var status =
      test.status === test.PASS
        ? "PASS"
        : test.status === test.TIMEOUT
          ? "TIMEOUT"
          : "FAIL";
    globalThis.__SPIKE_TESTS__.push({
      name: test.name || test.message || "unnamed",
      status: status,
    });
  });

  add_completion_callback(function (_tests, status) {
    var harness_status_label = function (st) {
      if (!st) return "UNKNOWN";
      if (st.status === st.OK) return "OK";
      if (st.status === st.ERROR) return "ERROR";
      if (st.status === st.TIMEOUT) return "TIMEOUT";
      if (st.status === st.PRECONDITION_FAILED) return "PRECONDITION_FAILED";
      return "NON_OK";
    };
    globalThis.__SPIKE_HARNESS__.status = harness_status_label(status);
    var pre = document.getElementById("__glide_results");
    if (!pre) {
      pre = document.createElement("pre");
      pre.id = "__glide_results";
      (document.body || document.documentElement).appendChild(pre);
    }
    pre.textContent =
      "__GLIDE_RESULTS__:" + JSON.stringify(globalThis.__SPIKE_HARNESS__);
  });
})();