/*
 CDXC:VoiceMode 2026-10-22 WHY:
 Read aloud's speaker on macOS (app/read_aloud/macos.rs): one AVSpeechSynthesizer for the whole
 app, owned by the main queue, with each segment handed over as one AVSpeechUtterance. The
 delegate's finish and cancel are what tell the chat core an utterance ended (`speechEnded`), so a
 reply's end is known without polling; `/usr/bin/say` was rejected for that reason (spec section
 8.1). Every utterance carries the token Rust gave it, and only the token speaking now is reported:
 an utterance stopped or replaced on purpose is never reported as cut off.
*/

#import <AVFoundation/AVFoundation.h>
#import <Foundation/Foundation.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef void (*GhostexReadAloudEndedCallback)(uint64_t token, int32_t interrupted);

@interface GhostexReadAloudSpeaker : NSObject <AVSpeechSynthesizerDelegate>
@property(nonatomic, strong) AVSpeechSynthesizer *synthesizer;
/* The token of each queued utterance. */
@property(nonatomic, strong) NSMapTable<AVSpeechUtterance *, NSNumber *> *tokens;
/* The last utterance of the token speaking now; nil once that token has been reported. */
@property(nonatomic, strong) AVSpeechUtterance *last;
@property(nonatomic) uint64_t token;
@property(nonatomic) GhostexReadAloudEndedCallback callback;
@end

@implementation GhostexReadAloudSpeaker

- (instancetype)init {
  self = [super init];
  if (self) {
    _synthesizer = [[AVSpeechSynthesizer alloc] init];
    _synthesizer.delegate = self;
    _tokens = [NSMapTable strongToStrongObjectsMapTable];
  }
  return self;
}

/* Reports the end of the token speaking now, once: its last utterance finished, or any of its
   utterances was cut off. Main queue only. */
- (void)utterance:(AVSpeechUtterance *)utterance endedInterrupted:(BOOL)interrupted {
  NSNumber *token = [self.tokens objectForKey:utterance];
  [self.tokens removeObjectForKey:utterance];
  if (token == nil || token.unsignedLongLongValue != self.token || self.last == nil) {
    return;
  }
  if (!interrupted && utterance != self.last) {
    return;
  }
  self.last = nil;
  if (self.callback != NULL) {
    self.callback(token.unsignedLongLongValue, interrupted ? 1 : 0);
  }
}

- (void)speechSynthesizer:(AVSpeechSynthesizer *)synthesizer
    didFinishSpeechUtterance:(AVSpeechUtterance *)utterance {
  dispatch_async(dispatch_get_main_queue(), ^{
    [self utterance:utterance endedInterrupted:NO];
  });
}

- (void)speechSynthesizer:(AVSpeechSynthesizer *)synthesizer
    didCancelSpeechUtterance:(AVSpeechUtterance *)utterance {
  dispatch_async(dispatch_get_main_queue(), ^{
    [self utterance:utterance endedInterrupted:YES];
  });
}

@end

static GhostexReadAloudSpeaker *GhostexReadAloudShared(void) {
  static GhostexReadAloudSpeaker *speaker = nil;
  if (speaker == nil) {
    speaker = [[GhostexReadAloudSpeaker alloc] init];
  }
  return speaker;
}

/* Speaks `count` texts in order under `token`, replacing anything speaking. `voice` is a voice
   identifier, or empty for the system voice; `rate` is an AVSpeechUtterance rate. `callback`
   hears the end on the main thread. Callable from any thread. */
void GhostexReadAloudSpeak(uint64_t token, const char *const *texts, size_t count,
                           const char *voice, float rate,
                           GhostexReadAloudEndedCallback callback) {
  NSMutableArray<NSString *> *strings = [NSMutableArray arrayWithCapacity:count];
  for (size_t index = 0; index < count; index++) {
    NSString *text = texts[index] != NULL ? [NSString stringWithUTF8String:texts[index]] : nil;
    if (text.length > 0) {
      [strings addObject:text];
    }
  }
  NSString *voiceIdentifier =
      (voice != NULL && voice[0] != '\0') ? [NSString stringWithUTF8String:voice] : nil;
  dispatch_async(dispatch_get_main_queue(), ^{
    GhostexReadAloudSpeaker *speaker = GhostexReadAloudShared();
    speaker.callback = callback;
    // The new token first, so the cancel of what was speaking is not reported.
    speaker.token = token;
    speaker.last = nil;
    [speaker.synthesizer stopSpeakingAtBoundary:AVSpeechBoundaryImmediate];
    AVSpeechSynthesisVoice *chosen =
        voiceIdentifier != nil ? [AVSpeechSynthesisVoice voiceWithIdentifier:voiceIdentifier] : nil;
    float clamped =
        MAX(AVSpeechUtteranceMinimumSpeechRate, MIN(AVSpeechUtteranceMaximumSpeechRate, rate));
    NSMutableArray<AVSpeechUtterance *> *utterances = [NSMutableArray array];
    for (NSString *text in strings) {
      AVSpeechUtterance *utterance = [AVSpeechUtterance speechUtteranceWithString:text];
      if (chosen != nil) {
        utterance.voice = chosen;
      }
      utterance.rate = clamped;
      [speaker.tokens setObject:@(token) forKey:utterance];
      [utterances addObject:utterance];
    }
    speaker.last = utterances.lastObject;
    if (speaker.last == nil) {
      // Nothing to say still ends, so the chat does not wait for ever.
      if (callback != NULL) {
        callback(token, 0);
      }
      return;
    }
    for (AVSpeechUtterance *utterance in utterances) {
      [speaker.synthesizer speakUtterance:utterance];
    }
  });
}

/* Stops whatever is speaking without reporting it. Callable from any thread. */
void GhostexReadAloudStop(void) {
  dispatch_async(dispatch_get_main_queue(), ^{
    GhostexReadAloudSpeaker *speaker = GhostexReadAloudShared();
    speaker.token = 0;
    speaker.last = nil;
    [speaker.synthesizer stopSpeakingAtBoundary:AVSpeechBoundaryImmediate];
  });
}

/* This Mac's voices as a JSON array of {id, name, language, languageName, quality, novelty},
   malloc'd; free it with GhostexReadAloudFree. */
char *GhostexReadAloudVoicesJson(void) {
  NSLocale *locale = [NSLocale currentLocale];
  NSMutableArray *voices = [NSMutableArray array];
  for (AVSpeechSynthesisVoice *voice in [AVSpeechSynthesisVoice speechVoices]) {
    NSString *languageName = [locale localizedStringForLocaleIdentifier:voice.language];
    BOOL novelty = NO;
    if (@available(macOS 14.0, *)) {
      novelty = (voice.voiceTraits & AVSpeechSynthesisVoiceTraitIsNoveltyVoice) != 0;
    }
    [voices addObject:@{
      @"id" : voice.identifier ?: @"",
      @"name" : voice.name ?: @"",
      @"language" : voice.language ?: @"",
      @"languageName" : languageName ?: (voice.language ?: @""),
      @"quality" : @((NSInteger)voice.quality),
      @"novelty" : @(novelty),
    }];
  }
  NSString *current = [AVSpeechSynthesisVoice currentLanguageCode] ?: @"";
  NSDictionary *answer = @{@"voices" : voices, @"currentLanguage" : current};
  NSData *data = [NSJSONSerialization dataWithJSONObject:answer options:0 error:nil];
  if (data == nil) {
    return NULL;
  }
  char *buffer = malloc(data.length + 1);
  if (buffer == NULL) {
    return NULL;
  }
  memcpy(buffer, data.bytes, data.length);
  buffer[data.length] = '\0';
  return buffer;
}

void GhostexReadAloudFree(char *buffer) { free(buffer); }
