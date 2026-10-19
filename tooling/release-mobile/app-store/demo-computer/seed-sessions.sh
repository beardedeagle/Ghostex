#!/bin/sh
# Run once after setup.sh and after supplying a temporary local Codex login.
set -eu

cd "$(dirname "$0")"
for machine in ghostex-appstore-demo ghostex-appstore-home; do
  docker exec -u demo "$machine" test -s /home/demo/.codex/auth.json || {
    echo "The temporary Codex login is missing in $machine." >&2
    exit 1
  }
  docker exec "$machine" sh -c \
    'printf "%s\n" "approval_policy = \"never\"" "sandbox_mode = \"danger-full-access\"" > /home/demo/.codex/config.toml && chown demo:demo /home/demo/.codex/config.toml'
done

project_id() {
  docker exec -u demo "$1" ghostex add-project "/home/demo/projects/$2" \
    --name "$3" --json | python3 -c 'import json,sys; print(json.load(sys.stdin)["project"]["projectId"])'
}

start_agent() {
  machine=$1
  project=$2
  title=$3
  prompt=$4
  session=$(docker exec -u demo "$machine" ghostex create-agent codex \
    --project-id "$project" --title "$title" --json | \
    python3 -c 'import json,sys; print(json.load(sys.stdin)["session"]["sessionId"])')
  docker exec -u demo "$machine" ghostex send-session-chat-message "$session" "$prompt" >/dev/null
  echo "$machine: $title ($session)"
}

atlas=$(project_id ghostex-appstore-demo atlas-api 'Atlas API')
side=$(project_id ghostex-appstore-demo side-project 'Side project')
wander=$(project_id ghostex-appstore-demo wander-travel 'Wander travel')
home_side=$(project_id ghostex-appstore-home side-project 'Side project')

start_agent ghostex-appstore-demo "$atlas" 'Fix flaky login test' \
  'Update tests/test_login.py to wait for the /dashboard URL instead of sleeping for two seconds. Keep the change small. Then explain the fix in two clear sentences.'
start_agent ghostex-appstore-demo "$side" 'Recipe app onboarding' \
  'Revise docs/onboarding.md into three welcoming steps for new cooks: choose food preferences, choose skill level, and pick a first recipe. Summarize the result briefly.'
start_agent ghostex-appstore-demo "$wander" 'Destination card direction' \
  'For the Wander travel page, decide how the destination cards should evolve. Before editing anything, ask me a multiple-choice question using your question tool: should the cards lead with a large destination photo or a compact itinerary map? Wait for my answer.'
start_agent ghostex-appstore-home "$home_side" 'Plan first recipe' \
  'Write docs/first-recipes.md with three beginner-friendly dinner ideas, each with one sentence about why it is approachable. Keep it short.'

start_agent ghostex-appstore-demo "$atlas" 'Auth error copy' \
  'Review the login error responses and write a short docs/auth-errors.md guide with clear user-facing wording for an expired session and invalid credentials.'
start_agent ghostex-appstore-demo "$atlas" 'Dashboard redirect check' \
  'Review the login redirect in this project and write a concise docs/dashboard-redirect.md checklist for manually verifying it.'
start_agent ghostex-appstore-demo "$side" 'First recipe shortlist' \
  'Write docs/first-recipes.md with three easy dinners for a new cook, each with one sentence explaining why it is approachable.'
start_agent ghostex-appstore-demo "$side" 'Shopping list flow' \
  'Write docs/shopping-list.md with four concise steps for turning a chosen recipe into a shopping list.'
start_agent ghostex-appstore-demo "$wander" 'Trip card layout' \
  'Write docs/trip-cards.md with a concise layout outline for the destination cards. Include a title, region, trip length, and one highlight.'
start_agent ghostex-appstore-demo "$wander" 'Preview launch checklist' \
  'Write docs/preview-checklist.md with a short checklist for reviewing the Wander travel page before a preview.'
start_agent ghostex-appstore-demo "$wander" 'Homepage direction' \
  'Before editing the Wander home page, ask me one multiple-choice question using your question tool: should the hero use a large destination photo or a compact itinerary map? Wait for my answer.'
