# Tokio Project Spell Checker Guide

## Introduction
This guide outlines the process for creating, maintaining and updating the spell checker for the Tokio project's documentation.
It was created from this request: [Add automated spell checking to Tokio documentation #6263 ](https://github.com/tokio-rs/tokio/issues/6263).

## 1. Identifying Files for Spell Checking

### Types of Files to Include:
- Markdown files (`.md`)
- Rust source files (`.rs`) for extracting `//!` and `///` documentation comments.

## 2. Using Hunspell for Spell Checking
The project uses [Hunspell](https://github.com/hunspell/hunspell) for spell checking. Configuration settings can be found in the `.spellcheck.toml` file.
### Installation
- Hunspell can be installed on most package managers. If you have to build it, theres guides on the github link.
- If you install through a package manager, you will need to download the dictionary you want to use. Those can be found [here](https://cgit.freedesktop.org/libreoffice/dictionaries/tree/)
### Mac/OSX
To install the en_US dictionary on a macbook, you can wget the following link.
```
wget https://cgit.freedesktop.org/libreoffice/dictionaries/tree/en/en_US.dic
wget https://cgit.freedesktop.org/libreoffice/dictionaries/tree/en/en_US.aff
mv en_US.dic ~/Library/Spelling/
mv en_us.aff ~/Library/Spelling/
```
#### TODO Add instructions for Linux
### Documentation
```
man 5 hunspell
man hunspell
hunspell -h
```
### Configuration
Hunspell is configured via the `.spellcheck.toml` file located in the root of the project.

#### Key Configuration Settings in `.spellcheck.toml`:
- **Language**: Specifies the language dictionary to use, e.g., `en_US` for American English.
- **Custom Dictionary**: The path to the custom dictionary file is specified here. For this project, it's `.spellcheck.dic`.
- **Search Dirs**: This setting determines where Hunspell looks for dictionary files.
- **Skip OS Lookups**: If set to `true`, Hunspell will not use the system's spell check dictionaries.
- **Use Builtin**: When enabled, Hunspell uses its built-in default dictionaries.

### Running Hunspell
- Run Hunspell by executing a command in the terminal. The specific command will depend on how it's integrated into the project. Typically, it would be something like `hunspell -d en_US -p .spellcheck.dic <file-to-check>`.
- You can check multiple files by listing them all or using wildcards.
```For a simple demo:
hunspell -d en_US
Hunspell 1.2.3
*
& exsample 4 0: example, examples, ex sample, ex-sample
```
For more in depth instruction, see the [man page](https://master.dl.sourceforge.net/project/hunspell/Hunspell/Documentation/hunspell1.pdf?viasf=1)

### Interpreting the Output
- Hunspell will list out any words it doesn't recognize from the files being checked.
- Words not found in either the standard or custom dictionary will be marked as potential errors.

### Updating the Custom Dictionary
- If a flagged word is valid (especially Tokio-specific terms or technical jargon), it should be added to the `.spellcheck.dic` file to prevent future flagging.
- The format for adding words to the custom dictionary should follow Hunspell's standards.


### Key Configuration Settings:
- Language: `en_US`
- Custom Dictionary: `.spellcheck.dic`

## 3. Building / Extending the Dictionary
### Parsing and Filtering Process:
- Extract words from the relevant files.
- Compare these words against a standard English dictionary.
- Flag words not found for review.

### Handling flagged words:
Theassumption is that the flagged words will be either technical words that are not included in the English lexicon,
or tokio-specific terms that are not included in the english lexicon.

#### Case 1
- In many cases, there can exist different spellings for a technical term.
  - Question: Should all spellings be included? Or should the community be asked so one spelling can be elected?
#### Case 2
- In the case of a tokio specific term, it should be easy to check the documentation to get the correct spelling.
#### Case: Non existing word
- If a word is made up or is just wrong, you should reach out to the original author to find out what was meant.

## 4. (Optional?) Automating the Process
The easiest way to build this dictionary is to write a script that will parse out all the words from relevant files,
and then run them against hunspell. All of this can be automated inside of a script. Is this script something
that should be included inside of the PR?

Considerations:
### 4.1 Suggested Scripting Languages:
- Python
- Shell script
### 4.2 Where should the script be located?
This is honestly more of a question.

### 5 Iterative Refinement
The spell checker dictionary should be refined iteratively.
Regardless if automation is included, how can we perform iterative refinement?

### Steps for Refinement:
Track all the files that documentation has been created for. When a new file is added, or has been changed:
- Review flagged words.
- Determine if they are correctly spelled technical terms.
- Update the dictionary accordingly.
Ideally this could all be done inside of a script.

## 6. Documentation and Guidelines
Maintain documentation for future reference.

### Key Points to Document:
- How to update the dictionary.
- How to run the spell checker.
- Guidelines for handling ambiguous cases.

## Conclusion
Regular updates and maintenance of the spell checker are essential for the accuracy and relevancy of the Tokio project documentation.


----------

# Questions
    0) Q) What should the branch name be?
       A)

    1) Technical Term Spellings:
        Q) For technical terms that may have multiple accepted spellings,
           should we include all variations in the dictionary, or should we consult the community/someone to
           decide on a preferred spelling?
        A)

    2) Script Inclusion in the PR:
        Q) Should the script used for automating the spell checker process be included in the pull request?
           More over, is there a preference for the language used? (e.g., Python, Bash, Rust)
           For example: update_dictionary.sh
        A)

    3) Location for the Automation Script:
        Q) If 2) is included, what would be the ideal location for this script within the project structure?
        a)

    4) Iterative Refinement Process:
        Q) How should we handle the iterative refinement of the spell checker dictionary?
           Specifically,  any specific suggestions or preferences for the process of reviewing and updating the dictionary
           based on flagged words from new or changed documentation files?
           For example, a PR introduces a tokio-specific word that doesnt exist in the dictionary.
           Should the developer add it to the dictionary as part of his PR? Or should it get automatically picked up in
           some process later on.
        A)

    5) Handling Tokio-specific Terms:
        Q) In cases where a term is specific to Tokio and not part of the standard English lexicon,
           what should be our approach for verifying and adding these terms to the .spellcheck.dic file?
        A)

    6) Dealing with Non-Existent or Incorrect Words:
        Q) When we encounter words that appear to be made-up or incorrect, what should be the protocol
           for reaching out to the original author for clarification?
        A)
