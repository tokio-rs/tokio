# Tokio Roadmap

## A Roadmap to 1.0

The question of "why not 1.0?" has come up a few times. After all, Tokio 0.1 has
been stable for three years. The short answer: because it isn't time. There is
nobody who would rather ship a Tokio 1.0 than us. It also isn't something to rush.

After all, `async / await` only landed in the stable Rust channel weeks ago.
There has been no significant production validation yet, except maybe fuchsia
and that seems like a fairly specialized use case. This release of Tokio
includes significant new code and new strategies with feature flags. Also, there
are still big open questions, such as the [proposed changes][pr-1744] to
`AsyncRead` and `AsyncWrite`.

Tokio 1.0 will be released as soon as the APIs are proven to handle real-world
production cases.

### Tokio 1.0 in Q3 2020 with LTS support

The Tokio 1.0 release will be **no later** than Q3 2020. It will also come with
"long-term support" guarantees:

* A minimum of 5 years of maintenance.
* A minimum of 3 years before a hypothetical 2.0 release.

When Tokio 1.0 is released in Q3 2020, on-going support, security fixes, and
critical bug fixes are guaranteed until **at least** Q3 2025. Tokio 2.0 will not
be released until **at least** Q3 2023 (though, ideally there will never be a
Tokio 2.0 release).

### How to get there

While Tokio 0.1 probably should have been a 1.0, Tokio 0.2 will be a **true**
0.2 release. There will be breaking change releases every 2 ~ 3 months until 1.0.
These changes will be **much** smaller than going from 0.1 -> 0.2. It is
expected that the 1.0 release will look a lot like 0.2.

### What is expected to change

The biggest change will be the `AsyncRead` and `AsyncWrite` traits. Based on
experience gained over the past 3 years, there are a couple of issues to
address:

* Be able to **safely** use uninitialized memory as a read buffer.
* Practical read vectored and write vectored APIs.

There are a few strategies to solve these problems. These strategies need to be
investigated and the solution validated. You can see [this comment][pr-1744-comment] for a
detailed statement of the problem.

The other major change, which has been in the works for a while, is updating
Mio. Mio 0.6 was first released almost 4 years ago and has not had a breaking
change since. Mio 0.7 has been in the works for a while. It includes a full
rewrite of the windows support as well as a refined API. More will be written
about this shortly.

Finally, now that the API is starting to stabilize, effort will be put into
documentation. Tokio 0.2 is being released before updating the website and many
of the old content will no longer be relevant. In the coming weeks, expect to
see updates there.

So, we have our work cut out for us. We hope you enjoy this 0.2 release and are
looking forward to your feedback and help.

[pr-1744]: https://github.com/tokio-rs/tokio/pull/1744
[pr-1744-comment]: https://github.com/tokio-rs/tokio/pull/1744#issuecomment-553575438
