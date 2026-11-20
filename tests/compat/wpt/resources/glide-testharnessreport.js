(function () {
  function harness_status_label(status) {
    if (!status) return "UNKNOWN";
    if (status.status === status.OK) return "OK";
    if (status.status === status.ERROR) return "ERROR";
    if (status.status === status.TIMEOUT) return "TIMEOUT";
    if (status.status === status.PRECONDITION_FAILED) return "PRECONDITION_FAILED";
    return "NON_OK";
  }

  function write_results(tests, status) {
    var summary = {
      status: harness_status_label(status),
      tests: tests.map(function (t) {
        return {
          name: t.name || t.message || "unnamed",
          status:
            t.status === t.PASS
              ? "PASS"
              : t.status === t.TIMEOUT
                ? "TIMEOUT"
                : "FAIL",
        };
      }),
    };
    var pre = document.getElementById("__glide_results");
    if (!pre) {
      pre = document.createElement("pre");
      pre.id = "__glide_results";
      document.body.appendChild(pre);
    }
    pre.textContent = "__GLIDE_RESULTS__:" + JSON.stringify(summary);
  }

  add_completion_callback(write_results);
})();