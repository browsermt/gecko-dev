/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

const { GeckoViewActorChild } = ChromeUtils.import(
  "resource://gre/modules/GeckoViewActorChild.jsm"
);
const { LoadURIDelegate } = ChromeUtils.import(
  "resource://gre/modules/LoadURIDelegate.jsm"
);

var EXPORTED_SYMBOLS = ["LoadURIDelegateChild"];

// Implements nsILoadURIDelegate.
class LoadURIDelegateChild extends GeckoViewActorChild {
  /** Returns true if this docShell is of type Content, false otherwise. */
  get isContentWindow() {
    if (!this.docShell) {
      return false;
    }

    return this.docShell.itemType == this.docShell.typeContent;
  }

  // nsILoadURIDelegate.
  loadURI(aUri, aWhere, aFlags, aTriggeringPrincipal) {
    debug`loadURI: uri=${aUri && aUri.spec}
                    where=${aWhere} flags=0x${aFlags.toString(16)}
                    tp=${aTriggeringPrincipal &&
                      aTriggeringPrincipal.URI &&
                      aTriggeringPrincipal.URI.spec}`;
    if (!this.isContentWindow) {
      debug`loadURI: not a content window`;
      // This is an internal Gecko window, nothing to do
      return;
    }

    return LoadURIDelegate.load(
      this.contentWindow,
      this.eventDispatcher,
      aUri,
      aWhere,
      aFlags,
      aTriggeringPrincipal
    );
  }

  // nsILoadURIDelegate.
  handleLoadError(aUri, aError, aErrorModule) {
    debug`handleLoadError: uri=${aUri && aUri.spec}
                             displaySpec=${aUri && aUri.displaySpec}
                             error=${aError}`;
    if (!this.isContentWindow) {
      // This is an internal Gecko window, nothing to do
      debug`handleLoadError: not a content window`;
      return;
    }

    if (aUri && LoadURIDelegate.isSafeBrowsingError(aError)) {
      const message = {
        type: "GeckoView:ContentBlocked",
        uri: aUri.spec,
        error: aError,
      };

      this.eventDispatcher.sendRequest(message);
    }

    return LoadURIDelegate.handleLoadError(
      this.contentWindow,
      this.eventDispatcher,
      aUri,
      aError,
      aErrorModule
    );
  }
}

LoadURIDelegateChild.prototype.QueryInterface = ChromeUtils.generateQI([
  Ci.nsILoadURIDelegate,
]);

const { debug, warn } = LoadURIDelegateChild.initLogging("LoadURIDelegate"); // eslint-disable-line no-unused-vars
