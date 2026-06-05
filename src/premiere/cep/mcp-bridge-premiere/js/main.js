(function () {
  var autoRunCheckbox = document.getElementById("autoRun");
  var statusEl = document.getElementById("status");
  var bridgeRootEl = document.getElementById("bridgeRoot");
  var commandFileEl = document.getElementById("commandFile");
  var resultFileEl = document.getElementById("resultFile");
  var logEl = document.getElementById("log");
  var autoRun = true;
  var pollInFlight = false;
  var pollTimer = null;
  var pollTimeoutTimer = null;

  function setText(el, text) {
    if (el) {
      el.textContent = text;
    }
  }

  function setLog(text) {
    if (logEl) {
      logEl.textContent = text;
    }
  }

  function showFatal(message) {
    if (statusEl) {
      statusEl.textContent = "Panel error";
    }
    setLog(message);
    if (!logEl) {
      document.body.innerHTML =
        '<div style="padding:12px;color:#e6e6e6;background:#1f1f1f;font:12px Segoe UI,Arial,sans-serif;">' +
        '<div style="font-weight:600;margin-bottom:8px;">Premiere MCP Bridge</div>' +
        '<pre style="white-space:pre-wrap;margin:0;">' + message.replace(/[&<>]/g, function (ch) {
          return ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[ch];
        }) + "</pre></div>";
    }
  }

  function evalScript(script, callback) {
    if (window.__adobe_cep__ && window.__adobe_cep__.evalScript) {
      window.__adobe_cep__.evalScript(script, callback || function () {});
    } else if (callback) {
      callback("");
    }
  }

  function updateState(raw) {
    forcePaint();
    if (!raw) {
      setText(statusEl, "Waiting");
      return;
    }
    var state;
    try {
      state = JSON.parse(raw);
    } catch (e) {
      setText(statusEl, "Invalid state");
      setLog("Failed to parse bridge state:\n" + raw);
      return;
    }
    if (typeof state.autoRun === "boolean") {
      autoRun = state.autoRun;
      if (autoRunCheckbox) {
        autoRunCheckbox.checked = autoRun;
      }
    }
    setText(statusEl, state.lastStatus || "idle");
    setText(bridgeRootEl, state.bridgeRoot || "-");
    setText(commandFileEl, state.commandFile || "-");
    setText(resultFileEl, state.resultFile || "-");

    var logLines = [];
    if (state.lastCommand) {
      logLines.push("Last command: " + state.lastCommand);
    }
    if (state.lastMessage) {
      logLines.push(state.lastMessage);
    }
    if (state.lastError) {
      logLines.push("Error: " + state.lastError);
    }
    if (state.lastRunAt) {
      logLines.push("Last run: " + state.lastRunAt);
    }
    setLog(logLines.length ? logLines.join("\n") : "Waiting for commands...");
  }

  function forcePaint() {
    if (!document.body) {
      return;
    }
    document.body.style.opacity = "0.999";
    if (document.body.offsetHeight >= 0) {
      document.body.style.opacity = "1";
    }
  }

  function pollOnce() {
    if (pollInFlight) {
      return;
    }
    pollInFlight = true;
    if (pollTimeoutTimer) {
      window.clearTimeout(pollTimeoutTimer);
    }
    pollTimeoutTimer = window.setTimeout(function () {
      pollInFlight = false;
      setText(statusEl, "Waiting for CEP");
    }, 5000);
    var callback = function (raw) {
      if (pollTimeoutTimer) {
        window.clearTimeout(pollTimeoutTimer);
        pollTimeoutTimer = null;
      }
      pollInFlight = false;
      updateState(raw);
    };
    if (autoRun) {
      evalScript("mcpBridgeCheck()", callback);
    } else {
      evalScript("mcpBridgeGetState()", callback);
    }
  }

  if (autoRunCheckbox) {
    autoRunCheckbox.addEventListener("change", function () {
      autoRun = autoRunCheckbox.checked;
      evalScript("mcpBridgeSetAutoRun(" + (autoRun ? "true" : "false") + ")", updateState);
    });
  }

  function init() {
    if (!statusEl || !bridgeRootEl || !commandFileEl || !resultFileEl || !logEl) {
      showFatal("Panel HTML did not initialize correctly.");
      return;
    }
    setText(statusEl, "Panel loaded");
    setLog("Panel loaded. Waiting for bridge state...");
    forcePaint();
    evalScript("mcpBridgeGetState()", updateState);
    if (!pollTimer) {
      pollTimer = window.setInterval(pollOnce, 2000);
    }
  }

  window.onerror = function (message, source, lineno) {
    showFatal("JavaScript error: " + message + "\n" + (source || "") + ":" + (lineno || 0));
  };

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
