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

  fcitx::SurroundingText middle_forward;
  middle_forward.setText("left middle right", 11, 5);
  assert(SelectedTextFromSurroundingText(middle_forward) == "middle");
  auto middle_forward_range = SelectedTextDeletionRange(middle_forward);
  assert(middle_forward_range.has_value());
  assert(middle_forward_range->offset == -6);
  assert(middle_forward_range->size == 6);

  fcitx::SurroundingText middle_backward;
  middle_backward.setText("left middle right", 5, 11);
  assert(SelectedTextFromSurroundingText(middle_backward) == "middle");
  auto middle_backward_range = SelectedTextDeletionRange(middle_backward);
  assert(middle_backward_range.has_value());
  assert(middle_backward_range->offset == 0);
  assert(middle_backward_range->size == 6);

  fcitx::SurroundingText utf8;
  utf8.setText("你好abc", 2, 0);
  assert(SelectedTextFromSurroundingText(utf8) == "你好");
  auto utf8_range = SelectedTextDeletionRange(utf8);
  assert(utf8_range.has_value());
  assert(utf8_range->offset == -2);
  assert(utf8_range->size == 2);

  fcitx::SurroundingText backward_utf8;
  backward_utf8.setText("ab你好", 2, 4);
  assert(SelectedTextFromSurroundingText(backward_utf8) == "你好");
  auto backward_utf8_range = SelectedTextDeletionRange(backward_utf8);
  assert(backward_utf8_range.has_value());
  assert(backward_utf8_range->offset == 0);
  assert(backward_utf8_range->size == 2);

  const std::string emoji = "\xF0\x9F\x98\x80";
  fcitx::SurroundingText emoji_text;
  emoji_text.setText("a" + emoji + "b", 2, 1);
  assert(SelectedTextFromSurroundingText(emoji_text) == emoji);
  auto emoji_range = SelectedTextDeletionRange(emoji_text);
  assert(emoji_range.has_value());
  assert(emoji_range->offset == -1);
  assert(emoji_range->size == 1);

  fcitx::SurroundingText collapsed;
  collapsed.setText("nothing selected", 7, 7);
  assert(SelectedTextFromSurroundingText(collapsed).empty());
  assert(!SelectedTextDeletionRange(collapsed).has_value());
  fcitx::SurroundingText full_forward;
  full_forward.setText("replace all", 11, 0);
  assert(SelectedTextFromSurroundingText(full_forward) == "replace all");
  auto full_forward_range = SelectedTextDeletionRange(full_forward);
  assert(full_forward_range.has_value());
  assert(full_forward_range->offset == -11);
  assert(full_forward_range->size == 11);

  fcitx::SurroundingText full_backward;
  full_backward.setText("replace all", 0, 11);
  assert(SelectedTextFromSurroundingText(full_backward) == "replace all");
  auto full_backward_range = SelectedTextDeletionRange(full_backward);
  assert(full_backward_range.has_value());
  assert(full_backward_range->offset == 0);
  assert(full_backward_range->size == 11);
  return 0;
}
