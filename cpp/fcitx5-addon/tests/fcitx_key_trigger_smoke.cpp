#include "vinput_fcitx_bridge/fcitx_key_trigger.h"

#include <fcitx-utils/key.h>
#include <fcitx/event.h>

#include <cassert>

using vinput_fcitx_bridge::FcitxKeyTriggerPolicy;

int main() {
  const FcitxKeyTriggerPolicy policy;

  fcitx::KeyEvent control_release(nullptr, fcitx::Key(FcitxKey_Control_R), true);
  assert(policy.IsNormalTrigger(control_release));

  fcitx::KeyEvent control_press(nullptr, fcitx::Key(FcitxKey_Control_R), false);
  assert(!policy.IsNormalTrigger(control_press));

  fcitx::KeyEvent shift_release(nullptr, fcitx::Key(FcitxKey_Shift_R), true);
  assert(!policy.IsNormalTrigger(shift_release));

  const FcitxKeyTriggerPolicy shift_policy{fcitx::Key(FcitxKey_Shift_R)};
  assert(shift_policy.IsNormalTrigger(shift_release));
  assert(!shift_policy.IsNormalTrigger(control_release));

  return 0;
}
