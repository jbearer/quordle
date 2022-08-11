use bitvec::{bitvec, vec::BitVec};
use clap::Parser;
use itertools::Itertools;
use rand::prelude::*;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
struct Options {
    #[clap(name = "WORDS")]
    words: PathBuf,
}

#[derive(Clone)]
struct Word {
    word: String,
    letters: BitVec,
    index: usize,
}

impl Word {
    fn new(index: usize, word: String) -> Self {
        Self {
            letters: word_letters(&word),
            index,
            word,
        }
    }
}

struct GroupNode {
    word: Word,
    parent: Option<Arc<GroupNode>>,
}

impl GroupNode {
    fn words(&self) -> Words {
        Words(Some(self))
    }
}

struct Words<'a>(Option<&'a GroupNode>);

impl<'a> Iterator for Words<'a> {
    type Item = &'a Word;

    fn next(&mut self) -> Option<&'a Word> {
        match self.0 {
            Some(node) => {
                self.0 = node.parent.as_ref().map(|a| &**a);
                Some(&node.word)
            }
            None => None,
        }
    }
}

struct WordGroup {
    length: usize,
    letters: BitVec,
    node: Arc<GroupNode>,
}

impl WordGroup {
    fn new(word: Word) -> Self {
        Self {
            length: 1,
            letters: word.letters.clone(),
            node: Arc::new(GroupNode { word, parent: None }),
        }
    }

    fn word(&self) -> &Word {
        &self.node.word
    }

    fn words(&self) -> impl Iterator<Item = &Word> {
        self.node.words()
    }

    fn add(&self, word: Word) -> Option<Self> {
        if (word.letters.clone() & self.letters.clone()).any() {
            return None;
        }
        Some(Self {
            length: 1 + self.length,
            letters: self.letters.clone() | word.letters.clone(),
            node: Arc::new(GroupNode {
                word,
                parent: Some(self.node.clone()),
            }),
        })
    }
}

impl Display for WordGroup {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        for w in self.words() {
            write!(f, "{} ", w.word)?;
        }
        Ok(())
    }
}

fn word_letters(word: &str) -> BitVec {
    let mut letters = bitvec![0; 26];
    for letter in word.chars() {
        letters.set(letter as usize - 'a' as usize, true);
    }
    letters
}

fn main() -> io::Result<()> {
    let opt = Options::parse();
    let words: Vec<String> = std::str::from_utf8(&fs::read(&opt.words)?)
        .unwrap()
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .filter(|w| {
            w.len() == 5
                && w.chars().all(|c| c.is_ascii_lowercase())
                && w.chars().unique().count() == 5
        })
        .unique()
        .collect();
    println!("Found {} 5-letter heterogrammic words", words.len());

    // Reduce to one representative of each anagrammic equivalence class. If there exists a
    // heterogrammic group including anagrams, then
    //  * the group contains no pairs of anagrams (else it would repeat letters)
    //  * swapping out any word in the group for an anagram of that word gives an equivalent group
    // Therefore a group of representatives of equivalences classes of words is itself a
    // representative of an equivalence class of groups, and we can recover the full class by
    // permuting the representatives of the anagram classes we include in the group.
    let mut anagrams: HashMap<BitVec, Vec<String>> = Default::default();
    for word in words {
        let letters = word_letters(&word);
        anagrams.entry(letters).or_default().push(word);
    }
    let words = anagrams.values().map(|v| v[0].clone()).collect::<Vec<_>>();
    println!("Found {} anagrammic equivalence classes", words.len());

    let words = words
        .into_iter()
        .enumerate()
        .map(|(i, w)| Word::new(i, w))
        .collect::<Vec<_>>();

    // Map the index of each word to the indices of all words _after it_ with which it is
    // heterogrammic. As long as we consider groups starting with each word, we only need to
    // consider heterogrammic words after a given word `w` when extending a group that contains `w`,
    // because if there is a word before `w` that extends the group, then the group itself is an
    // extension of another group, and we will find it that way.
    let heterogrammic: Vec<HashSet<usize>> = words
        .iter()
        .map(|word| {
            words[word.index + 1..]
                .iter()
                .filter_map(|w| {
                    if (word.letters.clone() & w.letters.clone()).not_any() {
                        Some(w.index)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .collect();

    // All groups of length 1: the singleton group for each word.
    let mut groups: Vec<WordGroup> = words.iter().cloned().map(WordGroup::new).collect();

    // Try to extend each group with all possible words, giving all groups of length `i + 1`. We
    // iterate this process to fixpoint. This is important, even if we are ultimately only
    // interested in groups of length 5, because we need to extend early groups as long as they will
    // go to ensure that we find all possible groups, since we only extend groups with words that
    // come later.
    for i in 1.. {
        println!("{} groups of length {}", groups.len(), i);
        if groups.is_empty() {
            break;
        }
        println!("here is a sampling:");
        for _ in 0..5 {
            println!("  {}", groups.choose(&mut thread_rng()).unwrap());
        }

        groups = groups
            .par_iter()
            .flat_map(|g| {
                // Find all words which might extend this group. To avoid the expense of trying to
                // extend the group with every word in the dictionary, we will first only consider
                // words which are heterogrammic with (and later than) the first word in the group,
                // and we will then filter this set of words even further by including only words
                // which are hterogrammic with (and later than) all other words which are already in
                // the group.
                let extensions = heterogrammic[g.word().index].par_iter().filter(|&&j| {
                    g.words().all(|w| j > w.index)
                        && g.words()
                            .skip(1)
                            .all(|w| heterogrammic[w.index].contains(&j))
                });
                extensions.filter_map(|&j| match g.add(words[j].clone()) {
                    Some(g) => Some(g),
                    None => None,
                })
            })
            .collect();
    }

    Ok(())
}
