window.qsoRipperDebug = window.qsoRipperDebug || {};

window.qsoRipperDebug.copyText = async function (text) {
    if (navigator.clipboard && window.isSecureContext) {
        await navigator.clipboard.writeText(text);
        return true;
    }

    const textArea = document.createElement("textarea");
    textArea.value = text;
    textArea.style.position = "fixed";
    textArea.style.opacity = "0";
    document.body.appendChild(textArea);
    textArea.focus();
    textArea.select();

    try {
        document.execCommand("copy");
        return true;
    } finally {
        document.body.removeChild(textArea);
    }
};
