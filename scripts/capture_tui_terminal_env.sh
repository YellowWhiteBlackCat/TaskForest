# Sourced by capture-tui.sh to freeze the exact signal environment passed to
# the TUI process inside Alacritty.

CAPTURE_TERMINAL_SCENARIO="${TM_TUI_CAPTURE_TERMINAL_SCENARIO:-<manual>}"
if [ "$CAPTURE_TERMINAL_SCENARIO" = "<manual>" ]; then
  CAPTURE_APP_TERM=alacritty
else
  CAPTURE_APP_TERM="${TERM:-<unset>}"
fi

capture_env_value() {
  local variable="$1"
  if [[ ${!variable+x} ]]; then
    printf '%s' "${!variable}"
  else
    printf '<unset>'
  fi
}

CAPTURE_APP_COLORTERM="$(capture_env_value COLORTERM)"
CAPTURE_APP_NO_COLOR="$(capture_env_value NO_COLOR)"
CAPTURE_APP_LC_ALL="$(capture_env_value LC_ALL)"
CAPTURE_APP_LC_CTYPE="$(capture_env_value LC_CTYPE)"
CAPTURE_APP_LC_MESSAGES="$(capture_env_value LC_MESSAGES)"
CAPTURE_APP_LANG="$(capture_env_value LANG)"
CAPTURE_APP_GLYPH_OVERRIDE="$(capture_env_value TM_TUI_GLYPHS)"

TUI_APP_ENV_UNSET=()
TUI_APP_ENV_SET=()
append_tui_env() {
  local variable="$1" value="$2"
  if [ "$value" = "<unset>" ]; then
    TUI_APP_ENV_UNSET+=(-u "$variable")
  else
    TUI_APP_ENV_SET+=("$variable=$value")
  fi
}

append_tui_env TERM "$CAPTURE_APP_TERM"
append_tui_env COLORTERM "$CAPTURE_APP_COLORTERM"
append_tui_env NO_COLOR "$CAPTURE_APP_NO_COLOR"
append_tui_env LC_ALL "$CAPTURE_APP_LC_ALL"
append_tui_env LC_CTYPE "$CAPTURE_APP_LC_CTYPE"
append_tui_env LC_MESSAGES "$CAPTURE_APP_LC_MESSAGES"
append_tui_env LANG "$CAPTURE_APP_LANG"
append_tui_env TM_TUI_GLYPHS "$CAPTURE_APP_GLYPH_OVERRIDE"
TUI_APP_ENV_UNSET+=(-u TM_TUI_CAPTURE_TERMINAL_SCENARIO -u TM_TUI_CAPTURE_PREBUILT_BINARY)
