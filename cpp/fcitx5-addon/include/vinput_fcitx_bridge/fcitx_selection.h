#pragma once

#include <optional>
#include <string>

namespace fcitx {
class SurroundingText;
}

namespace vinput_fcitx_bridge {

struct SurroundingTextSelectionRange {
  int offset = 0;
  unsigned int size = 0;
};

std::optional<SurroundingTextSelectionRange>
SelectedTextDeletionRange(const fcitx::SurroundingText &surrounding_text);

std::string
SelectedTextFromSurroundingText(const fcitx::SurroundingText &surrounding_text);

} // namespace vinput_fcitx_bridge
