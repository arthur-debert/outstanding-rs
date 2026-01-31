(function () {
  if (window.__standoutVersionBanner) {
    return;
  }
  window.__standoutVersionBanner = true;

  var menuBar = document.getElementById("menu-bar");
  if (!menuBar) {
    return;
  }

  var path = window.location.pathname || "/";
  var parts = path.split("/").filter(function (part) {
    return part.length > 0;
  });

  if (parts.length === 0) {
    return;
  }

  var version = parts[0];
  var isLatest = version === "latest";
  var isTagged = /^standout-v\d+\.\d+\.\d+$/.test(version);

  if (!isLatest && !isTagged) {
    return;
  }

  if (document.querySelector(".version-banner")) {
    return;
  }

  var banner = document.createElement("div");
  banner.className = "version-banner";
  banner.setAttribute("role", "status");

  var label = document.createElement("span");
  label.className = "version-banner__label";
  label.textContent = "Docs version";

  var pill = document.createElement("span");
  pill.className = "version-banner__pill";
  pill.textContent = version;

  var message = document.createElement("span");
  message.className = "version-banner__message";
  message.textContent = isLatest
    ? "You're viewing the latest docs."
    : "You're viewing an older version.";

  var links = document.createElement("span");
  links.className = "version-banner__links";

  if (!isLatest) {
    var latestLink = document.createElement("a");
    latestLink.className = "version-banner__link";
    latestLink.href = "/latest/";
    latestLink.textContent = "Go to latest";

    var separator = document.createElement("span");
    separator.className = "version-banner__separator";
    separator.textContent = "•";

    links.appendChild(latestLink);
    links.appendChild(separator);
  }

  var versionsLink = document.createElement("a");
  versionsLink.className = "version-banner__link";
  versionsLink.href = "/";
  versionsLink.textContent = "All versions";

  links.appendChild(versionsLink);

  banner.appendChild(label);
  banner.appendChild(pill);
  banner.appendChild(message);
  banner.appendChild(links);

  menuBar.insertAdjacentElement("afterend", banner);
})();
