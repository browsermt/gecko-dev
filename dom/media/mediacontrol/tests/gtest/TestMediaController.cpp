/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#include "gtest/gtest.h"
#include "MediaControlService.h"
#include "MediaController.h"

using namespace mozilla::dom;

#define CONTROLLER_ID 0

TEST(MediaController, DefaultValueCheck)
{
  RefPtr<MediaController> controller = new MediaController(CONTROLLER_ID);
  ASSERT_TRUE(controller->ControlledMediaNum() == 0);
  ASSERT_TRUE(controller->Id() == CONTROLLER_ID);
  ASSERT_TRUE(controller->GetState() == PlaybackState::eStopped);
  ASSERT_TRUE(!controller->IsAudible());
}

TEST(MediaController, NotifyMediaStateChanged)
{
  RefPtr<MediaController> controller = new MediaController(CONTROLLER_ID);
  ASSERT_TRUE(controller->ControlledMediaNum() == 0);

  controller->NotifyMediaStateChanged(ControlledMediaState::eStarted);
  ASSERT_TRUE(controller->ControlledMediaNum() == 1);

  controller->NotifyMediaStateChanged(ControlledMediaState::eStarted);
  ASSERT_TRUE(controller->ControlledMediaNum() == 2);

  controller->NotifyMediaStateChanged(ControlledMediaState::eStopped);
  ASSERT_TRUE(controller->ControlledMediaNum() == 1);

  controller->NotifyMediaStateChanged(ControlledMediaState::eStopped);
  ASSERT_TRUE(controller->ControlledMediaNum() == 0);
}

TEST(MediaController, ActiveAndDeactiveController)
{
  RefPtr<MediaControlService> service = MediaControlService::GetService();
  ASSERT_TRUE(service->GetControllersNum() == 0);

  RefPtr<MediaController> controller1 =
      new MediaController(FIRST_CONTROLLER_ID);

  controller1->NotifyMediaStateChanged(ControlledMediaState::eStarted);
  ASSERT_TRUE(service->GetControllersNum() == 1);

  controller1->NotifyMediaStateChanged(ControlledMediaState::eStopped);
  ASSERT_TRUE(service->GetControllersNum() == 0);
}

TEST(MediaController, AudibleChanged)
{
  RefPtr<MediaController> controller = new MediaController(CONTROLLER_ID);
  controller->Play();
  ASSERT_TRUE(!controller->IsAudible());

  controller->NotifyMediaAudibleChanged(true);
  ASSERT_TRUE(controller->IsAudible());

  controller->NotifyMediaAudibleChanged(false);
  ASSERT_TRUE(!controller->IsAudible());
}

TEST(MediaController, AlwaysInaudibleIfControllerIsNotPlaying)
{
  RefPtr<MediaController> controller = new MediaController(CONTROLLER_ID);
  ASSERT_TRUE(!controller->IsAudible());

  controller->NotifyMediaAudibleChanged(true);
  ASSERT_TRUE(!controller->IsAudible());

  controller->Play();
  ASSERT_TRUE(controller->IsAudible());

  controller->Pause();
  ASSERT_TRUE(!controller->IsAudible());

  controller->Play();
  ASSERT_TRUE(controller->IsAudible());

  controller->Stop();
  ASSERT_TRUE(!controller->IsAudible());
}

TEST(MediaController, ChangePlayingStateViaPlayPauseStop)
{
  RefPtr<MediaController> controller = new MediaController(CONTROLLER_ID);
  ASSERT_TRUE(controller->GetState() == PlaybackState::eStopped);

  controller->Play();
  ASSERT_TRUE(controller->GetState() == PlaybackState::ePlaying);

  controller->Pause();
  ASSERT_TRUE(controller->GetState() == PlaybackState::ePaused);

  controller->Play();
  ASSERT_TRUE(controller->GetState() == PlaybackState::ePlaying);

  controller->Stop();
  ASSERT_TRUE(controller->GetState() == PlaybackState::eStopped);
}

class FakeControlledMedia final {
 public:
  explicit FakeControlledMedia(MediaController* aController)
      : mController(aController) {
    mController->NotifyMediaStateChanged(ControlledMediaState::eStarted);
  }

  void SetPlaying(bool aIsPlaying) {
    if (mIsPlaying == aIsPlaying) {
      return;
    }
    mController->NotifyMediaStateChanged(aIsPlaying
                                             ? ControlledMediaState::ePlayed
                                             : ControlledMediaState::ePaused);
    mIsPlaying = aIsPlaying;
  }

  ~FakeControlledMedia() {
    if (mIsPlaying) {
      mController->NotifyMediaStateChanged(ControlledMediaState::ePaused);
    }
    mController->NotifyMediaStateChanged(ControlledMediaState::eStopped);
  }

 private:
  bool mIsPlaying = false;
  RefPtr<MediaController> mController;
};

TEST(MediaController, PlayingStateChangeViaControlledMedia)
{
  RefPtr<MediaController> controller = new MediaController(CONTROLLER_ID);

  // In order to check playing state after FakeControlledMedia destroyed.
  {
    FakeControlledMedia foo(controller);
    ASSERT_TRUE(controller->GetState() == PlaybackState::eStopped);

    foo.SetPlaying(true);
    ASSERT_TRUE(controller->GetState() == PlaybackState::ePlaying);

    foo.SetPlaying(false);
    ASSERT_TRUE(controller->GetState() == PlaybackState::ePaused);

    foo.SetPlaying(true);
    ASSERT_TRUE(controller->GetState() == PlaybackState::ePlaying);
  }

  // FakeControlledMedia has been destroyed, no playing media exists.
  ASSERT_TRUE(controller->GetState() == PlaybackState::ePaused);
}

TEST(MediaController, ControllerShouldRemainPlayingIfAnyPlayingMediaExists)
{
  RefPtr<MediaController> controller = new MediaController(CONTROLLER_ID);

  {
    FakeControlledMedia foo(controller);
    ASSERT_TRUE(controller->GetState() == PlaybackState::eStopped);

    foo.SetPlaying(true);
    ASSERT_TRUE(controller->GetState() == PlaybackState::ePlaying);

    // foo is playing, so controller is in `playing` state.
    FakeControlledMedia bar(controller);
    ASSERT_TRUE(controller->GetState() == PlaybackState::ePlaying);

    bar.SetPlaying(true);
    ASSERT_TRUE(controller->GetState() == PlaybackState::ePlaying);

    // Although we paused bar, but foo is still playing, so the controller would
    // still be in `playing`.
    bar.SetPlaying(false);
    ASSERT_TRUE(controller->GetState() == PlaybackState::ePlaying);

    foo.SetPlaying(false);
    ASSERT_TRUE(controller->GetState() == PlaybackState::ePaused);
  }

  // both foo and bar have been destroyed, no playing media exists.
  ASSERT_TRUE(controller->GetState() == PlaybackState::ePaused);
}
