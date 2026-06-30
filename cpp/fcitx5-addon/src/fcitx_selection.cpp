#include "vinput_fcitx_bridge/fcitx_selection.h"

#include <fcitx/surroundingtext.h>

namespace vinput_fcitx_bridge {

std::string
SelectedTextFromSurroundingText(const fcitx::SurroundingText &surrounding_text) {
  if (!surrounding_text.isValid()) {
    return {};
  }
  return surrounding_text.selectedText();
}

} // namespace vinput_fcitx_bridge
