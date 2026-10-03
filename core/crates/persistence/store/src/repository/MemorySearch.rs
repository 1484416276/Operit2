//! Kotlin RRF/coverage/semantic/graph search, independent of storage and UI.
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use operit_model::Memory::{Memory,MemoryLink};
use operit_model::MemorySearchConfig::{MemorySearchConfig,MemoryScoreMode};
use operit_model::MemorySearchDebugInfo::{MemorySearchDebugInfo,MemorySearchDebugCandidate};

pub fn lexicalTokens(query: &str) -> Vec<String> {
    static JIEBA: OnceLock<jieba_rs::Jieba> = OnceLock::new();
    let jieba = JIEBA.get_or_init(jieba_rs::Jieba::new);
    let keywords = keywords(query);
    let mut tokens = HashSet::new();
    for keyword in &keywords {
        let word = keyword.trim().to_lowercase();
        if keepToken(&word) { tokens.insert(word.clone()); }
        if !word.contains('*') {
            for token in jieba.cut(&word,false) {
                if keepToken(token) { tokens.insert(token.to_string()); }
            }
        }
    }
    let mut tokens = tokens.into_iter().collect::<Vec<_>>();
    tokens.sort_by(|a,b|b.chars().count().cmp(&a.chars().count()).then_with(||a.cmp(b)));
    tokens.truncate(32); tokens
}
fn keepToken(word: &str) -> bool {
    let word=word.replace('*',"");
    (2..=24).contains(&word.chars().count()) && word.chars().any(char::is_alphanumeric)
}
pub fn matches(text: &str, token: &str) -> bool {
    if token.contains('*') {
        let pattern = token.split('*').filter(|s|!s.is_empty()).map(regex::escape).collect::<Vec<_>>().join(".*");
        regex::RegexBuilder::new(&pattern).case_insensitive(true).dot_matches_new_line(true).build()
            .map(|r|r.is_match(text)).unwrap_or(false)
    } else { text.to_lowercase().contains(&token.to_lowercase()) }
}
pub fn keywords(query:&str)->Vec<String> {
    if query.contains('|') { query.split('|').map(str::trim).filter(|s|!s.is_empty()).map(ToString::to_string).collect() }
    else { query.split_whitespace().map(ToString::to_string).collect() }
}

/// semantic similarities are per keyword and optional; callers own the embedding backend/cache.
pub fn compute(query:&str, memories:&[Memory], links:&[MemoryLink], config:MemorySearchConfig,
    threshold:f64, semantic:&[Vec<(i64,f32)>]) -> MemorySearchDebugInfo {
    let config=config.normalized(); let tokens=lexicalTokens(query); let keywords=keywords(query);
    let (kw,sw,ew)=match config.scoreMode { MemoryScoreMode::BALANCED=>(1.0,1.0,1.0),
        MemoryScoreMode::KEYWORD_FIRST=>(1.3,0.8,0.9),MemoryScoreMode::SEMANTIC_FIRST=>(0.8,1.3,1.1) };
    let kw=f64::from(config.keywordWeight)*kw; let tw=f64::from(config.tagWeight)*match config.scoreMode { MemoryScoreMode::KEYWORD_FIRST=>1.3,MemoryScoreMode::SEMANTIC_FIRST=>0.8,_=>1.0 };
    let sw=config.vectorWeight*sw as f32; let ew=f64::from(config.edgeWeight)*ew;
    let norm=1.0/(keywords.len().max(1) as f64).sqrt();
    let mut rows=HashMap::<i64,MemorySearchDebugCandidate>::new();
    let byId=memories.iter().map(|m|(m.id,m)).collect::<HashMap<_,_>>();
    let row = |m:&Memory| MemorySearchDebugCandidate { memoryId:m.id,title:m.title.clone(),folderPath:m.folderPath.clone(),..Default::default() };
    let mut counts=[0i32;3];
    for kind in 0..3 {
        let mut found=memories.iter().filter_map(|m| {
            if m.title == ".folder_placeholder" { return None; }
            let count=tokens.iter().filter(|t|if kind==1 {m.tags.iter().any(|tag|matches(&tag.name,t))} else {matches(&m.title,t)}).count();
            if kind==2 {
                let title=m.title.trim().to_lowercase();
                if !title.is_empty() && query.to_lowercase().contains(&title) { Some((m,1usize)) } else { None }
            } else if count>0 { Some((m,count)) } else { None }
        }).collect::<Vec<_>>();
        found.sort_by(|(a,ac),(b,bc)|bc.cmp(ac).then_with(||b.importance.total_cmp(&a.importance)).then_with(||b.updatedAt.cmp(&a.updatedAt)));
        counts[kind]=found.len() as i32;
        if kind==2 { found.sort_by(|(a,_),(b,_)|b.importance.total_cmp(&a.importance).then_with(||b.updatedAt.cmp(&a.updatedAt))); }
        let weight=if kind==1 {tw}else{kw};
        if weight<=0.0 {continue;}
        for (index,(m,count)) in found.iter().enumerate() {
            let coverage=if kind==2 {1.0}else{1.0+0.6*(*count as f64)/(tokens.len().max(1) as f64)};
            let score=1.0/(61.0+index as f64)*f64::from(m.importance)*weight*coverage;
            let part=rows.entry(m.id).or_insert_with(||row(m));
            match kind {0=>part.keywordScore+=score,1=>part.tagScore+=score,_=>part.reverseContainmentScore+=score};
            part.matchedKeywordTokenCount=part.matchedKeywordTokenCount.max(*count as i32);
        }
    }
    let mut semanticIds=HashSet::new();
    if sw>0.0 {for found in semantic {
        let mut found=found.iter().filter(|(id,_)|byId.contains_key(id)).collect::<Vec<_>>();
        found.sort_by(|a,b|b.1.total_cmp(&a.1));
        for (index,(id,similarity)) in found.into_iter().enumerate() {
            let m=byId[id];
            let score=(1.0/(61.0+index as f64)*f64::from(m.importance.max(0.0)).sqrt()+f64::from(*similarity*sw))*norm;
            rows.entry(*id).or_insert_with(||row(m)).semanticScore+=score;semanticIds.insert(*id);
        }
    }}
    for part in rows.values_mut() {part.totalScore=part.keywordScore+part.tagScore+part.reverseContainmentScore+part.semanticScore;}
    let mut seeds=rows.values().map(|p|(p.memoryId,p.totalScore)).collect::<Vec<_>>();
    seeds.sort_by(|a,b|b.1.total_cmp(&a.1));seeds.truncate(10);
    let mut edges=0;
    if ew>0.0 {for (id,_) in seeds {
        let sourceScore=rows[&id].totalScore;
        for link in links {let target=if link.sourceMemoryId==id {Some(link.targetMemoryId)} else if link.targetMemoryId==id {Some(link.sourceMemoryId)} else {None};
            if let Some(m)=target.and_then(|id|byId.get(&id).copied()) {
                let boost=sourceScore*f64::from(link.weight)*ew+0.03*ew;
                let part=rows.entry(m.id).or_insert_with(||row(m));part.edgeScore+=boost;part.totalScore+=boost;edges+=1;
            }
        }
    }}
    let mut candidates=rows.into_values().collect::<Vec<_>>();
    for c in &mut candidates {c.passedThreshold=c.totalScore>=threshold.max(0.0);}
    candidates.sort_by(|a,b|b.totalScore.total_cmp(&a.totalScore).then_with(||a.memoryId.cmp(&b.memoryId)));
    let finalResultIds=candidates.iter().filter(|p|p.passedThreshold).map(|p|p.memoryId).collect::<Vec<_>>();
    MemorySearchDebugInfo {query:query.into(),keywords,lexicalTokens:tokens,scoreMode:config.scoreMode,relevanceThreshold:threshold.max(0.0),
        effectiveKeywordWeight:kw,effectiveTagWeight:tw,effectiveSemanticWeight:sw,semanticKeywordNormFactor:norm,effectiveEdgeWeight:ew,
        memoriesInScopeCount:memories.len() as i32,keywordMatchesCount:counts[0],tagMatchesCount:counts[1],reverseContainmentMatchesCount:counts[2],
        semanticMatchesCount:semanticIds.len() as i32,graphEdgesTraversed:edges,scoredCount:candidates.len() as i32,
        passedThresholdCount:finalResultIds.len() as i32,candidates,finalResultIds }
}

/// Chunk lexical/semantic search excludes tags, reverse-title and graph channels, as in Kotlin.
pub fn computeChunks(query:&str,chunks:&[operit_model::DocumentChunk::DocumentChunk],config:MemorySearchConfig,
    semantic:&[Vec<(i64,f32)>])->Vec<i64> {
    let config=config.normalized();let tokens=lexicalTokens(query);
    let (km,sm)=match config.scoreMode {MemoryScoreMode::BALANCED=>(1.0,1.0),MemoryScoreMode::KEYWORD_FIRST=>(1.3,0.8),MemoryScoreMode::SEMANTIC_FIRST=>(0.8,1.3)};
    let mut scores=HashMap::<i64,f64>::new();
    let mut lexical=chunks.iter().filter_map(|c| {let count=tokens.iter().filter(|t|matches(&c.content,t)).count();(count>0).then_some((c,count))}).collect::<Vec<_>>();
    lexical.sort_by(|(a,ac),(b,bc)|bc.cmp(ac).then_with(||a.chunkIndex.cmp(&b.chunkIndex)));
    if config.keywordWeight>0.0 {for (rank,(c,count)) in lexical.into_iter().enumerate() {
        scores.insert(c.id, f64::from(config.keywordWeight)*km*(1.0+0.6*count as f64/tokens.len().max(1) as f64)/(61.0+rank as f64));
    }}
    let norm=1.0/(keywords(query).len().max(1) as f64).sqrt();
    if config.vectorWeight>0.0 {for found in semantic {
        let mut found=found.clone();found.sort_by(|a,b|b.1.total_cmp(&a.1));
        for (rank,(id,similarity)) in found.into_iter().enumerate() {
            *scores.entry(id).or_default()+=(1.0/(61.0+rank as f64)+f64::from(similarity*config.vectorWeight)*sm)*norm;
        }
    }}
    let mut scored=scores.into_iter().collect::<Vec<_>>();scored.sort_by(|a,b|b.1.total_cmp(&a.1).then_with(||a.0.cmp(&b.0)));
    scored.into_iter().map(|(id,_)|id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn chinese_and_wildcard_tokens() {
        assert!(lexicalTokens("用户喜欢咖啡").iter().any(|t|t=="咖啡"));
        assert!(matches("SSH connection works", "ssh*works"));
        assert!(!matches("tmux", "ssh*works"));
    }
}
