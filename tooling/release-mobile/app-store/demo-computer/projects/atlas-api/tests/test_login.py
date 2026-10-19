import time


def test_login_redirect(page):
    page.goto("http://localhost:3000/login")
    page.get_by_role("button", name="Sign in").click()
    time.sleep(2)
    assert page.url.endswith("/dashboard")
