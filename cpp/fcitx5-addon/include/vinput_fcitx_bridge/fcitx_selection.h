#pragma once

#include <string>

namespace fcitx {
class SurroundingText;
}

namespace vinput_fcitx_bridge {

std::string
SelectedTextFromSurroundingText(const fcitx::SurroundingText &surrounding_text);

} // namespace vinput_fcitx_bridge
