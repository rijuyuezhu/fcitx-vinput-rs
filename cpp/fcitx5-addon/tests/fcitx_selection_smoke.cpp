#include "vinput_fcitx_bridge/fcitx_selection.h"

#include <fcitx/surroundingtext.h>

#include <cassert>
#include <string>

using vinput_fcitx_bridge::SelectedTextFromSurroundingText;

int main() {
  fcitx::SurroundingText invalid;
  assert(SelectedTextFromSurroundingText(invalid).empty());

  fcitx::SurroundingText forward;
  forward.setText("selected tail", 8, 0);
  assert(SelectedTextFromSurroundingText(forward) == "selected");

  fcitx::SurroundingText backward;
  backward.setText("head selected", 5, 13);
  assert(SelectedTextFromSurroundingText(backward) == "selected");

  fcitx::SurroundingText collapsed;
  collapsed.setText("nothing selected", 7, 7);
  assert(SelectedTextFromSurroundingText(collapsed).empty());

  return 0;
}
