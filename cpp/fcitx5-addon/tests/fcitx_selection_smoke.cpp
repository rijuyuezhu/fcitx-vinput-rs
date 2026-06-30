#include "vinput_fcitx_bridge/fcitx_selection.h"

#include <fcitx/surroundingtext.h>

#include <cassert>
#include <string>

using vinput_fcitx_bridge::SelectedTextDeletionRange;
using vinput_fcitx_bridge::SelectedTextFromSurroundingText;

int main() {
  fcitx::SurroundingText invalid;
  assert(SelectedTextFromSurroundingText(invalid).empty());
  assert(!SelectedTextDeletionRange(invalid).has_value());

  fcitx::SurroundingText forward;
  forward.setText("selected tail", 8, 0);
  assert(SelectedTextFromSurroundingText(forward) == "selected");
  auto forward_range = SelectedTextDeletionRange(forward);
  assert(forward_range.has_value());
  assert(forward_range->offset == -8);
  assert(forward_range->size == 8);

  fcitx::SurroundingText backward;
  backward.setText("head selected", 5, 13);
  assert(SelectedTextFromSurroundingText(backward) == "selected");
  auto backward_range = SelectedTextDeletionRange(backward);
  assert(backward_range.has_value());
  assert(backward_range->offset == 0);
  assert(backward_range->size == 8);

  fcitx::SurroundingText utf8;
  utf8.setText("你好abc", 2, 0);
  assert(SelectedTextFromSurroundingText(utf8) == "你好");
  auto utf8_range = SelectedTextDeletionRange(utf8);
  assert(utf8_range.has_value());
  assert(utf8_range->offset == -2);
  assert(utf8_range->size == 2);

  fcitx::SurroundingText collapsed;
  collapsed.setText("nothing selected", 7, 7);
  assert(SelectedTextFromSurroundingText(collapsed).empty());
  assert(!SelectedTextDeletionRange(collapsed).has_value());

  return 0;
}
