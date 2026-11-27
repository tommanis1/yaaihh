"""Whether a page is a gate a person must pass: a CAPTCHA or anti-bot check, a sign-in, an institutional sign-in.
The rule reads the driver's signals {url, title, frames, status, headers, text, password}; the browser tools stop at
the first gate and hand the page to the owner. Nothing here solves, clicks or waits out a gate."""
import re, urllib.parse

CAPTCHA_HOSTS = {  # a frame from one of these is a challenge
    "challenges.cloudflare.com": "Cloudflare Turnstile", "www.google.com/recaptcha": "reCAPTCHA",
    "www.recaptcha.net": "reCAPTCHA", "recaptcha.net": "reCAPTCHA", "hcaptcha.com": "hCaptcha",
    "newassets.hcaptcha.com": "hCaptcha", "client-api.arkoselabs.com": "Arkose", "funcaptcha.com": "Arkose",
    "geo.captcha-delivery.com": "DataDome", "captcha.px-cdn.net": "PerimeterX", "js.hcaptcha.com": "hCaptcha"}
CHECK_WORDS = re.compile(r"just a moment|checking your browser|verify you are (a )?human|are you a robot|"
                         r"attention required! \| cloudflare|unusual traffic|press (and|&) hold|captcha", re.I)
SSO = re.compile(r"(^|\.)(login\.microsoftonline\.com|accounts\.google\.com|[\w-]+\.okta\.com|openathens\.net|"
                 r"[\w-]+\.auth0\.com|login\.ezproxy\.[\w.]+|shibboleth\.[\w.]+|idp\.[\w.]+|sso\.[\w.]+|wayf\.[\w.]+|"
                 r"login\.[\w.]+|auth\.[\w.]+)$", re.I)
INST_PATH = re.compile(r"/(idp/profile/saml2|saml2?/|shibboleth\.sso|cas/login|wayf|openathens)(\b|/|$)", re.I)
LOGIN_PATH = re.compile(r"/(oauth2?/authorize|signin|sign-in|login)(\b|/|$)", re.I)


def frame_captcha(url):
    u = urllib.parse.urlsplit(url)
    host, path = (u.hostname or "").lower(), u.path.lower()
    for h, name in CAPTCHA_HOSTS.items():
        hh, _, hp = h.partition("/")
        if (host == hh or host.endswith("." + hh)) and path.startswith("/" + hp if hp else "/"):
            return name
    return None


def reason(sig):
    """Why a person must take the page (one line), or None."""
    for f in sig.get("frames") or []:
        name = frame_captcha(f)
        if name:
            return f"{name} challenge"
    headers = sig.get("headers") or {}
    if headers.get("cf-mitigated") == "challenge":
        return "Cloudflare challenge"
    title, text, status = sig.get("title") or "", (sig.get("text") or "")[:3000], sig.get("status")
    if CHECK_WORDS.search(title) or (status in (403, 429, 503) and CHECK_WORDS.search(text)):
        return "anti-bot check"
    if status in (401, 407):
        return f"sign-in (HTTP {status})"
    u = urllib.parse.urlsplit(sig.get("url") or "")
    if SSO.search(u.hostname or "") or INST_PATH.search(u.path or ""):
        return f"institutional sign-in ({u.hostname})"
    if sig.get("password"):
        return f"sign-in form ({u.hostname})"
    if LOGIN_PATH.search(u.path or ""):
        return f"sign-in page ({u.hostname})"
    return None
